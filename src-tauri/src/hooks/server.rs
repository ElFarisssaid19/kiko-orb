//! Local hook server: 127.0.0.1 only, one POST endpoint per hook event, every request carrying the
//! secret token. Permissions and AskUserQuestion calls are held open until the user decides in the
//! island, chooses to reply in the terminal, or the timeout passes.

use std::{
    collections::VecDeque,
    io::{self, Read},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use serde_json::{Map, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use super::{
    events::{AgentEvent, HookLogEntry, ResolveReason, SessionRef, StepStatus},
    install::{HOOK_PATH_PREFIX, TOKEN_HEADER},
    parse::{self, ASK_USER_QUESTION, Hook, PermissionDecision},
};

/// Lines kept for the playground's debug log.
const LOG_SIZE: usize = 50;
/// Write payloads carry whole files; anything bigger is refused.
const MAX_BODY_BYTES: u64 = 16 * 1024 * 1024;

/// What the server reports to the app.
#[derive(Clone, Debug)]
pub enum Emitted {
    Event(AgentEvent),
    Log(HookLogEntry),
}

/// The user's answer to a held request.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum UserResponse {
    Permission {
        decision: PermissionDecision,
    },
    /// Question text → chosen label (several labels joined with ", ").
    Question {
        answers: Map<String, Value>,
    },
    /// Reply in the terminal: Kiko answers without a decision and Claude Code shows its own prompt.
    Release,
}

enum HeldKind {
    Permission { always_updates: Vec<Value> },
    Question { tool_input: Value },
}

/// A hook request kept open until the user decides.
struct Held {
    id: String,
    session_id: String,
    since: Instant,
    kind: HeldKind,
    request: Request,
    /// The event that announced it, replayed to windows that open later.
    event: AgentEvent,
}

struct Shared {
    token: String,
    decision_timeout: Duration,
    /// Oldest first.
    held: Mutex<Vec<Held>>,
    log: Mutex<VecDeque<HookLogEntry>>,
    next_id: AtomicU64,
    emit: Box<dyn Fn(Emitted) + Send + Sync>,
}

#[derive(Clone)]
pub struct HookServer {
    shared: Arc<Shared>,
    port: u16,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Compares secrets in constant time.
fn same_secret(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn reply(request: Request, status: u16) {
    // Claude Code may already have given up on the request (timeout, Ctrl+C): nothing to do then.
    let _ = request.respond(Response::empty(status));
}

fn reply_json(request: Request, body: &Value) {
    let header =
        Header::from_bytes("Content-Type", "application/json").expect("static header is valid");
    let _ = request.respond(Response::from_string(body.to_string()).with_header(header));
}

impl HookServer {
    /// Binds 127.0.0.1:`port` (0 = any free port) and serves hooks on background threads.
    pub fn start(
        port: u16,
        token: String,
        decision_timeout: Duration,
        emit: impl Fn(Emitted) + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let server = Server::http(("127.0.0.1", port)).map_err(|error| {
            match error.downcast::<io::Error>() {
                Ok(error) => *error,
                Err(error) => io::Error::other(error),
            }
        })?;
        let port = server
            .server_addr()
            .to_ip()
            .map_or(port, |addr| addr.port());
        let shared = Arc::new(Shared {
            token,
            decision_timeout,
            held: Mutex::new(Vec::new()),
            log: Mutex::new(VecDeque::with_capacity(LOG_SIZE)),
            next_id: AtomicU64::new(1),
            emit: Box::new(emit),
        });

        let worker = Arc::clone(&shared);
        thread::Builder::new()
            .name("kiko-hooks".into())
            .spawn(move || {
                for request in server.incoming_requests() {
                    worker.handle(request);
                }
            })?;
        let reaper = Arc::clone(&shared);
        thread::Builder::new()
            .name("kiko-hooks-timeout".into())
            .spawn(move || {
                loop {
                    thread::sleep(Duration::from_millis(250));
                    reaper.release_where(
                        |held| held.since.elapsed() >= reaper.decision_timeout,
                        ResolveReason::Timeout,
                    );
                }
            })?;

        Ok(Self { shared, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Sends the user's answer back to Claude Code.
    pub fn respond(&self, request_id: &str, response: UserResponse) -> Result<(), String> {
        let held = {
            let mut held = lock(&self.shared.held);
            let index = held
                .iter()
                .position(|h| h.id == request_id)
                .ok_or("This request is no longer waiting.")?;
            let matches = matches!(
                (&held[index].kind, &response),
                (HeldKind::Permission { .. }, UserResponse::Permission { .. })
                    | (HeldKind::Question { .. }, UserResponse::Question { .. })
                    | (_, UserResponse::Release)
            );
            if !matches {
                return Err("This answer does not fit the request.".into());
            }
            held.remove(index)
        };
        let (body, reason) = match (&held.kind, response) {
            (HeldKind::Permission { always_updates }, UserResponse::Permission { decision }) => (
                Some(parse::permission_response(decision, always_updates)),
                ResolveReason::Answered,
            ),
            (HeldKind::Question { tool_input }, UserResponse::Question { answers }) => (
                Some(parse::question_response(tool_input, &answers)),
                ResolveReason::Answered,
            ),
            _ => (None, ResolveReason::Released),
        };
        self.shared.finish(held, body, reason);
        Ok(())
    }

    /// Events of the requests still waiting, oldest first.
    pub fn pending_events(&self) -> Vec<AgentEvent> {
        lock(&self.shared.held)
            .iter()
            .map(|held| held.event.clone())
            .collect()
    }

    pub fn log(&self) -> Vec<HookLogEntry> {
        lock(&self.shared.log).iter().cloned().collect()
    }
}

impl Shared {
    fn emit(&self, event: AgentEvent) {
        (self.emit)(Emitted::Event(event));
    }

    fn log(&self, event: &str, session: &SessionRef, summary: String) {
        let entry = HookLogEntry {
            at: now_ms(),
            event: event.to_string(),
            session_id: session.id.clone(),
            project: session.project.clone(),
            summary,
        };
        {
            let mut log = lock(&self.log);
            if log.len() == LOG_SIZE {
                log.pop_front();
            }
            log.push_back(entry.clone());
        }
        (self.emit)(Emitted::Log(entry));
    }

    fn authorized(&self, request: &Request) -> bool {
        request
            .headers()
            .iter()
            .find(|header| header.field.equiv(TOKEN_HEADER))
            .is_some_and(|header| {
                same_secret(header.value.as_str().as_bytes(), self.token.as_bytes())
            })
    }

    fn handle(&self, mut request: Request) {
        let path = request.url().split('?').next().unwrap_or("");
        let Some(hook) = path
            .strip_prefix(HOOK_PATH_PREFIX)
            .and_then(Hook::from_path)
        else {
            return reply(request, 404);
        };
        if request.method() != &Method::Post {
            return reply(request, 405);
        }
        // Browsers send an Origin header and Claude Code doesn't: no web page gets to talk to Kiko.
        let from_browser = request
            .headers()
            .iter()
            .any(|header| header.field.equiv("Origin"));
        if from_browser || !self.authorized(&request) {
            let unknown = SessionRef {
                id: String::new(),
                project: String::new(),
                cwd: String::new(),
            };
            self.log(
                "Rejected",
                &unknown,
                format!("{} request without a valid token", hook.event_name()),
            );
            return reply(request, 401);
        }
        let mut body = String::new();
        if request
            .as_reader()
            .take(MAX_BODY_BYTES)
            .read_to_string(&mut body)
            .is_err()
        {
            return reply(request, 400);
        }
        match serde_json::from_str::<Value>(&body) {
            Ok(input) => self.on_hook(hook, &input, request),
            Err(_) => reply(request, 400),
        }
    }

    fn on_hook(&self, hook: Hook, input: &Value, request: Request) {
        let at = now_ms();
        let session = parse::session_ref(input);
        let tool = parse::tool_name(input);
        let text = |key: &str| {
            input
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        self.log(hook.event_name(), &session, parse::summary(hook, input));

        match hook {
            Hook::SessionStart => {
                self.emit(AgentEvent::SessionStarted {
                    at,
                    session,
                    source: text("source"),
                });
                reply(request, 200);
            }
            Hook::SessionEnd => {
                self.release_where(
                    |held| held.session_id == session.id,
                    ResolveReason::SessionEnded,
                );
                self.emit(AgentEvent::SessionEnded {
                    at,
                    session,
                    reason: text("reason"),
                });
                reply(request, 200);
            }
            Hook::UserPromptSubmit => {
                self.release_where(
                    |held| held.session_id == session.id,
                    ResolveReason::TurnEnded,
                );
                self.emit(AgentEvent::PromptSubmitted { at, session });
                reply(request, 200);
            }
            Hook::PreToolUse if tool == ASK_USER_QUESTION => {
                let step = parse::step(input, StepStatus::Running);
                self.emit(AgentEvent::ToolStarted {
                    at,
                    session: session.clone(),
                    step,
                });
                let id = self.next_id();
                let question = parse::question_request(&id, input);
                let event = AgentEvent::QuestionAsked {
                    at,
                    session,
                    request: question,
                };
                self.hold(
                    id,
                    input,
                    HeldKind::Question {
                        tool_input: parse::tool_input(input).clone(),
                    },
                    request,
                    event,
                );
            }
            Hook::PreToolUse => {
                let step = parse::step(input, StepStatus::Running);
                self.emit(AgentEvent::ToolStarted { at, session, step });
                reply(request, 200);
            }
            Hook::PostToolUse => {
                let (lines_added, lines_removed) =
                    parse::line_changes(tool, parse::tool_input(input));
                let step = parse::step(input, StepStatus::Done);
                self.emit(AgentEvent::ToolFinished {
                    at,
                    session,
                    step,
                    lines_added,
                    lines_removed,
                });
                reply(request, 200);
            }
            Hook::PostToolUseFailure => {
                let step = parse::step(input, StepStatus::Error);
                self.emit(AgentEvent::ToolFinished {
                    at,
                    session,
                    step,
                    lines_added: 0,
                    lines_removed: 0,
                });
                reply(request, 200);
            }
            // AskUserQuestion is answered through its PreToolUse; never show it as a permission.
            Hook::PermissionRequest if tool == ASK_USER_QUESTION => reply(request, 200),
            Hook::PermissionRequest => {
                let id = self.next_id();
                let permission = parse::permission_request(&id, input);
                let event = AgentEvent::PermissionRequested {
                    at,
                    session,
                    request: permission,
                };
                let always_updates = parse::always_allow_updates(input);
                self.hold(
                    id,
                    input,
                    HeldKind::Permission { always_updates },
                    request,
                    event,
                );
            }
            Hook::Notification => {
                self.emit(AgentEvent::Notification {
                    at,
                    session,
                    notification_type: text("notification_type"),
                    message: text("message"),
                });
                reply(request, 200);
            }
            Hook::Stop => {
                self.release_where(
                    |held| held.session_id == session.id,
                    ResolveReason::TurnEnded,
                );
                self.emit(AgentEvent::TurnStopped { at, session });
                reply(request, 200);
            }
            Hook::StopFailure => {
                self.release_where(
                    |held| held.session_id == session.id,
                    ResolveReason::TurnEnded,
                );
                self.emit(AgentEvent::TurnFailed {
                    at,
                    session,
                    error: text("error"),
                });
                reply(request, 200);
            }
        }
    }

    fn next_id(&self) -> String {
        format!("r{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn hold(&self, id: String, input: &Value, kind: HeldKind, request: Request, event: AgentEvent) {
        let session_id = parse::session_ref(input).id;
        lock(&self.held).push(Held {
            id,
            session_id,
            since: Instant::now(),
            kind,
            request,
            event: event.clone(),
        });
        self.emit(event);
    }

    /// Lets go of the held requests matching `which`, without a decision.
    fn release_where(&self, which: impl Fn(&Held) -> bool, reason: ResolveReason) {
        let released: Vec<Held> = {
            let mut held = lock(&self.held);
            let (out, keep) = held.drain(..).partition(|h| which(h));
            *held = keep;
            out
        };
        for held in released {
            self.finish(held, None, reason);
        }
    }

    /// Answers a held request (no body = no decision) and tells every window it is gone.
    fn finish(&self, held: Held, body: Option<Value>, reason: ResolveReason) {
        let Held {
            id,
            session_id,
            request,
            ..
        } = held;
        let outcome = match &body {
            Some(body) => body["hookSpecificOutput"]["decision"]["behavior"]
                .as_str()
                .unwrap_or("answered")
                .to_string(),
            None => format!("{reason:?}").to_lowercase(),
        };
        match body {
            Some(body) => reply_json(request, &body),
            None => reply(request, 200),
        }
        let session = SessionRef {
            id: session_id,
            project: String::new(),
            cwd: String::new(),
        };
        self.log("Kiko", &session, format!("{id}: {outcome}"));
        self.emit(AgentEvent::RequestResolved {
            at: now_ms(),
            request_id: id,
            reason,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Write, net::TcpStream, sync::mpsc};

    use serde_json::json;

    use super::*;

    const TOKEN: &str = "test-token-0123456789abcdef0123456789";

    fn start(timeout: Duration) -> (HookServer, mpsc::Receiver<AgentEvent>) {
        let (sender, receiver) = mpsc::channel();
        let sender = Mutex::new(sender);
        let server = HookServer::start(0, TOKEN.into(), timeout, move |emitted| {
            if let Emitted::Event(event) = emitted {
                let _ = lock(&sender).send(event);
            }
        })
        .unwrap();
        (server, receiver)
    }

    /// Minimal HTTP/1.1 client: (status, body).
    fn post(port: u16, path: &str, token: Option<&str>, body: &Value) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let body = body.to_string();
        let token = token
            .map(|t| format!("X-Kiko-Token: {t}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n{token}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let status = response[9..12].parse().unwrap();
        let body = response
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default();
        (status, body)
    }

    fn hook_post(server: &HookServer, hook: &str, body: &Value) -> (u16, String) {
        post(
            server.port(),
            &format!("/kiko/hooks/{hook}"),
            Some(TOKEN),
            body,
        )
    }

    /// PermissionRequest input from the hooks reference, for a given session.
    fn permission_input(session: &str) -> Value {
        json!({
          "session_id": session,
          "transcript_path": "/Users/.../.claude/projects/.../00893aaf-19fa-41d2-8238-13269b9b3ca0.jsonl",
          "cwd": "/Users/...",
          "permission_mode": "default",
          "hook_event_name": "PermissionRequest",
          "tool_name": "Bash",
          "tool_input": { "command": "rm -rf node_modules", "description": "Remove node_modules directory" },
          "permission_suggestions": [{
            "type": "addRules",
            "rules": [{ "toolName": "Bash", "ruleContent": "rm -rf node_modules" }],
            "behavior": "allow",
            "destination": "localSettings"
          }]
        })
    }

    fn wait_for_pending(server: &HookServer, count: usize) -> Vec<String> {
        for _ in 0..200 {
            let pending: Vec<String> = server
                .pending_events()
                .into_iter()
                .filter_map(|event| match event {
                    AgentEvent::PermissionRequested { request, .. } => Some(request.id),
                    AgentEvent::QuestionAsked { request, .. } => Some(request.id),
                    _ => None,
                })
                .collect();
            if pending.len() == count {
                return pending;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("expected {count} pending requests");
    }

    #[test]
    fn requests_need_the_token() {
        let (server, _events) = start(Duration::from_secs(60));
        let body = json!({ "session_id": "abc123", "cwd": "/x", "hook_event_name": "Stop" });
        assert_eq!(post(server.port(), "/kiko/hooks/stop", None, &body).0, 401);
        assert_eq!(
            post(server.port(), "/kiko/hooks/stop", Some("wrong"), &body).0,
            401
        );
        assert_eq!(
            post(server.port(), "/kiko/hooks/nope", Some(TOKEN), &body).0,
            404
        );
        assert_eq!(
            post(server.port(), "/kiko/hooks/stop", Some(TOKEN), &body).0,
            200
        );
    }

    #[test]
    fn observed_events_answer_at_once_with_an_empty_body() {
        let (server, events) = start(Duration::from_secs(60));
        // Common input example from the hooks reference.
        let input = json!({
          "session_id": "abc123",
          "cwd": "/home/user/my-project",
          "permission_mode": "default",
          "hook_event_name": "PreToolUse",
          "tool_name": "Bash",
          "tool_input": { "command": "npm test", "description": "Run test suite", "timeout": 120000, "run_in_background": false },
          "tool_use_id": "toolu_01ABC123..."
        });
        assert_eq!(
            hook_post(&server, "pre-tool-use", &input),
            (200, String::new())
        );
        match events.recv_timeout(Duration::from_secs(2)).unwrap() {
            AgentEvent::ToolStarted { session, step, .. } => {
                assert_eq!(session.project, "my-project");
                assert_eq!(step.target, "npm test");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn a_permission_waits_for_the_decision() {
        let (server, _events) = start(Duration::from_secs(60));
        let client = {
            let server = server.clone();
            thread::spawn(move || {
                hook_post(&server, "permission-request", &permission_input("abc123"))
            })
        };
        let ids = wait_for_pending(&server, 1);
        server
            .respond(
                &ids[0],
                UserResponse::Permission {
                    decision: PermissionDecision::Always,
                },
            )
            .unwrap();
        let (status, body) = client.join().unwrap();
        assert_eq!(status, 200);
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            body["hookSpecificOutput"]["hookEventName"],
            "PermissionRequest"
        );
        assert_eq!(body["hookSpecificOutput"]["decision"]["behavior"], "allow");
        assert_eq!(
            body["hookSpecificOutput"]["decision"]["updatedPermissions"][0]["type"],
            "addRules"
        );
        assert!(
            server.respond(&ids[0], UserResponse::Release).is_err(),
            "already answered"
        );
    }

    #[test]
    fn requests_queue_oldest_first_and_a_turn_end_drops_them() {
        let (server, _events) = start(Duration::from_secs(60));
        let first = {
            let server = server.clone();
            thread::spawn(move || {
                hook_post(&server, "permission-request", &permission_input("one"))
            })
        };
        wait_for_pending(&server, 1);
        let second = {
            let server = server.clone();
            thread::spawn(move || {
                hook_post(&server, "permission-request", &permission_input("two"))
            })
        };
        let ids = wait_for_pending(&server, 2);
        assert_eq!(ids, ["r1", "r2"]);

        // Session "one" stops: its request is let go without a decision; "two" keeps waiting.
        hook_post(
            &server,
            "stop",
            &json!({ "session_id": "one", "cwd": "/x", "hook_event_name": "Stop" }),
        );
        assert_eq!(first.join().unwrap(), (200, String::new()));
        assert_eq!(wait_for_pending(&server, 1), ["r2"]);

        server.respond("r2", UserResponse::Release).unwrap();
        assert_eq!(second.join().unwrap(), (200, String::new()));
    }

    #[test]
    fn an_unanswered_request_is_let_go_after_the_timeout() {
        let (server, events) = start(Duration::from_millis(300));
        let client = {
            let server = server.clone();
            thread::spawn(move || {
                hook_post(&server, "permission-request", &permission_input("abc123"))
            })
        };
        assert_eq!(client.join().unwrap(), (200, String::new()));
        let resolved = events
            .iter()
            .find_map(|event| match event {
                AgentEvent::RequestResolved { reason, .. } => Some(reason),
                _ => None,
            })
            .unwrap();
        assert_eq!(resolved, ResolveReason::Timeout);
    }

    #[test]
    fn a_question_is_answered_through_pre_tool_use() {
        let (server, _events) = start(Duration::from_secs(60));
        let input = json!({
            "session_id": "abc123",
            "cwd": "/home/user/my-project",
            "hook_event_name": "PreToolUse",
            "tool_name": "AskUserQuestion",
            "tool_input": { "questions": [{
                "question": "Which framework?", "header": "Framework",
                "options": [{"label": "React", "description": "Component library"}, {"label": "Vue", "description": "Progressive framework"}],
                "multiSelect": false
            }]},
            "tool_use_id": "toolu_02"
        });
        let client = {
            let server = server.clone();
            thread::spawn(move || hook_post(&server, "pre-tool-use", &input))
        };
        let ids = wait_for_pending(&server, 1);
        let mut answers = Map::new();
        answers.insert("Which framework?".into(), json!("React"));
        server
            .respond(&ids[0], UserResponse::Question { answers })
            .unwrap();
        let body: Value = serde_json::from_str(&client.join().unwrap().1).unwrap();
        assert_eq!(body["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(
            body["hookSpecificOutput"]["updatedInput"]["answers"]["Which framework?"],
            "React"
        );
        assert_eq!(
            body["hookSpecificOutput"]["updatedInput"]["questions"][0]["header"],
            "Framework"
        );
    }
}
