//! Runs the real kiko-channel binary the way Claude Code does (stdio MCP, session id in the
//! environment), against a fake Kiko server that speaks the channel protocol.

use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use kiko_channel::protocol::{
    CHANNEL_PATH_PREFIX, ChatMessage, Endpoint, Inbox, Register, Reply, TOKEN_HEADER,
};
use serde_json::{Value, json};
use tiny_http::{Header, Response, Server};

const TOKEN: &str = "test-token-0123456789abcdef0123456789";
const SESSION: &str = "abc123";
const WAIT: Duration = Duration::from_secs(10);

/// What the fake Kiko saw.
#[derive(Debug)]
enum Seen {
    Register(Register),
    Poll,
    Reply(Reply),
    Unregister,
    BadToken,
}

/// A stand-in for Kiko's server: records requests, answers polls with the messages queued by the test.
struct FakeKiko {
    seen: Receiver<Seen>,
    outbox: Sender<ChatMessage>,
}

impl FakeKiko {
    /// On a port of its own choosing: tests running in parallel can't take it first.
    fn start_any() -> (FakeKiko, u16) {
        let server = Server::http(("127.0.0.1", 0)).unwrap();
        let port = server.server_addr().to_ip().unwrap().port();
        (FakeKiko::serve(server), port)
    }

    /// On a port picked earlier, for a Kiko that starts after the channel.
    fn start(port: u16) -> FakeKiko {
        FakeKiko::serve(Server::http(("127.0.0.1", port)).unwrap())
    }

    fn serve(server: Server) -> FakeKiko {
        let server = Arc::new(server);
        let (seen_tx, seen) = mpsc::channel();
        let (outbox, outbox_rx) = mpsc::channel::<ChatMessage>();
        let outbox_rx = Arc::new(Mutex::new(outbox_rx));
        thread::spawn(move || {
            for mut request in server.incoming_requests() {
                let (seen_tx, outbox_rx) = (seen_tx.clone(), Arc::clone(&outbox_rx));
                // One thread per request: a held poll must not block the reply endpoint.
                thread::spawn(move || {
                    let token_ok = request
                        .headers()
                        .iter()
                        .any(|h| h.field.equiv(TOKEN_HEADER) && h.value.as_str() == TOKEN);
                    if !token_ok {
                        let _ = seen_tx.send(Seen::BadToken);
                        let _ = request.respond(Response::empty(401));
                        return;
                    }
                    let mut body = String::new();
                    request.as_reader().read_to_string(&mut body).unwrap();
                    let path = request.url().strip_prefix(CHANNEL_PATH_PREFIX).unwrap_or("").to_string();
                    let answer = match Endpoint::from_path(&path) {
                        Some(Endpoint::Register) => {
                            let _ = seen_tx.send(Seen::Register(serde_json::from_str(&body).unwrap()));
                            json!({})
                        }
                        Some(Endpoint::Poll) => {
                            let _ = seen_tx.send(Seen::Poll);
                            let messages = outbox_rx
                                .lock()
                                .unwrap()
                                .recv_timeout(Duration::from_millis(300))
                                .into_iter()
                                .collect();
                            serde_json::to_value(Inbox { messages }).unwrap()
                        }
                        Some(Endpoint::Reply) => {
                            let _ = seen_tx.send(Seen::Reply(serde_json::from_str(&body).unwrap()));
                            json!({})
                        }
                        Some(Endpoint::Unregister) => {
                            let _ = seen_tx.send(Seen::Unregister);
                            json!({})
                        }
                        None => {
                            let _ = request.respond(Response::empty(404));
                            return;
                        }
                    };
                    let header = Header::from_bytes("Content-Type", "application/json").unwrap();
                    let _ = request.respond(Response::from_string(answer.to_string()).with_header(header));
                });
            }
        });
        FakeKiko { seen, outbox }
    }

    /// The next request matching `pick`, skipping the others (mostly polls).
    fn expect<T>(&self, mut pick: impl FnMut(Seen) -> Option<T>) -> T {
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.seen.recv_timeout(left) {
                Ok(seen) => {
                    if let Some(found) = pick(seen) {
                        return found;
                    }
                }
                Err(RecvTimeoutError::Timeout) => panic!("the fake Kiko never got the expected request"),
                Err(RecvTimeoutError::Disconnected) => panic!("fake Kiko stopped"),
            }
        }
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn write_config(dir: &Path, port: u16) -> std::path::PathBuf {
    let path = dir.join("config.json");
    std::fs::write(
        &path,
        json!({ "port": port, "token": TOKEN, "decisionTimeoutSecs": 110 }).to_string(),
    )
    .unwrap();
    path
}

/// kiko-channel started like Claude Code starts a stdio MCP server.
struct Channel {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Value>,
    /// Messages read while waiting for something else.
    stash: Vec<Value>,
    next_id: u64,
}

impl Channel {
    fn spawn(config: Option<&Path>, session: Option<&str>) -> Channel {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kiko-channel"));
        command
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env("CLAUDE_PROJECT_DIR", "/home/user/my-project")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(config) = config {
            command.arg("--config").arg(config);
        }
        if let Some(session) = session {
            command.env("CLAUDE_CODE_SESSION_ID", session);
        }
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let _ = tx.send(serde_json::from_str::<Value>(&line).expect("stdout is JSON lines"));
            }
        });
        Channel { child, stdin, lines, stash: Vec::new(), next_id: 1 }
    }

    fn send(&mut self, message: Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    fn next_matching(&mut self, pick: impl Fn(&Value) -> bool) -> Value {
        if let Some(index) = self.stash.iter().position(&pick) {
            return self.stash.remove(index);
        }
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let message = self.lines.recv_timeout(left).expect("kiko-channel went quiet");
            if pick(&message) {
                return message;
            }
            self.stash.push(message);
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        self.next_matching(|m| m["id"] == id)
    }

    fn handshake(&mut self) -> Value {
        let init = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": { "roots": {} },
                "clientInfo": { "name": "claude-code", "version": "2.1.296" }
            }),
        );
        self.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        init
    }

    fn reply(&mut self, text: &str) -> Value {
        self.request("tools/call", json!({ "name": "reply", "arguments": { "text": text } }))
    }

    fn still_running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn handshake_declares_the_channel_and_the_reply_tool() {
    let mut channel = Channel::spawn(None, None);
    // Claude Code's v2 runtime probes for revision 2026-07-28 first; a channel must stay on `initialize`.
    let probe = channel.request("server/discover", json!({}));
    assert_eq!(probe["error"]["code"], -32601);

    let init = channel.handshake()["result"].clone();
    assert_eq!(init["protocolVersion"], "2025-06-18");
    assert_eq!(init["capabilities"]["experimental"]["claude/channel"], json!({}));
    assert_eq!(init["capabilities"]["tools"], json!({}));
    assert_eq!(init["serverInfo"]["name"], "kiko");
    assert!(init["instructions"].as_str().unwrap().contains("reply tool"));

    let tools = channel.request("tools/list", json!({}))["result"]["tools"].clone();
    assert_eq!(tools.as_array().unwrap().len(), 1);
    assert_eq!(tools[0]["name"], "reply");
    assert_eq!(tools[0]["inputSchema"]["required"], json!(["text"]));
    assert_eq!(channel.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn a_message_from_kiko_becomes_a_channel_event() {
    let dir = tempfile::tempdir().unwrap();
    let (kiko, port) = FakeKiko::start_any();
    let mut channel = Channel::spawn(Some(&write_config(dir.path(), port)), Some(SESSION));

    let registered = kiko.expect(|seen| match seen {
        Seen::Register(register) => Some(register),
        _ => None,
    });
    assert_eq!(registered.session_id, SESSION);
    assert_eq!(registered.cwd, "/home/user/my-project");
    assert_eq!(registered.pid, channel.child.id());
    // Started by this test like Claude Code starts it: the parent is the "Claude Code" process.
    assert_eq!(registered.parent_pid, std::process::id());
    assert_eq!(registered.channel.len(), 32);

    channel.handshake();
    kiko.outbox
        .send(ChatMessage { id: "m1".into(), text: "What does `npm test` say?".into(), at: 1 })
        .unwrap();
    let event = channel.next_matching(|m| m["method"] == "notifications/claude/channel");
    assert_eq!(event["params"]["content"], "What does `npm test` say?");
    assert_eq!(event["params"]["meta"]["message_id"], "m1");
}

#[test]
fn events_wait_for_the_end_of_the_handshake() {
    let dir = tempfile::tempdir().unwrap();
    let (kiko, port) = FakeKiko::start_any();
    let mut channel = Channel::spawn(Some(&write_config(dir.path(), port)), Some(SESSION));
    kiko.expect(|seen| matches!(seen, Seen::Register(_)).then_some(()));
    kiko.outbox.send(ChatMessage { id: "m1".into(), text: "early".into(), at: 1 }).unwrap();
    // Delivered to the channel by a poll, but held until `notifications/initialized`.
    kiko.expect(|seen| matches!(seen, Seen::Poll).then_some(()));
    assert!(channel.lines.recv_timeout(Duration::from_millis(800)).is_err());

    channel.handshake();
    let event = channel.next_matching(|m| m["method"] == "notifications/claude/channel");
    assert_eq!(event["params"]["content"], "early");
}

#[test]
fn the_reply_tool_posts_to_kiko() {
    let dir = tempfile::tempdir().unwrap();
    let (kiko, port) = FakeKiko::start_any();
    let mut channel = Channel::spawn(Some(&write_config(dir.path(), port)), Some(SESSION));
    let registered = kiko.expect(|seen| match seen {
        Seen::Register(register) => Some(register),
        _ => None,
    });
    channel.handshake();

    let answer = channel.reply("All 12 tests pass.\n\n```sh\nnpm test\n```");
    assert_eq!(answer["result"]["isError"], false);
    assert_eq!(answer["result"]["content"][0]["text"], "Sent to Kiko.");
    let reply = kiko.expect(|seen| match seen {
        Seen::Reply(reply) => Some(reply),
        _ => None,
    });
    assert_eq!(reply.channel, registered.channel);
    assert_eq!(reply.text, "All 12 tests pass.\n\n```sh\nnpm test\n```");
}

#[test]
fn without_kiko_the_session_keeps_working_and_connects_later() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port(); // nothing listens there yet
    let mut channel = Channel::spawn(Some(&write_config(dir.path(), port)), Some(SESSION));

    // MCP answers at once, Kiko or not.
    let started = Instant::now();
    assert_eq!(channel.handshake()["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(channel.request("tools/list", json!({}))["result"]["tools"][0]["name"], "reply");
    assert!(started.elapsed() < Duration::from_secs(2));

    // The reply tool tells Claude instead of failing the call.
    let answer = channel.reply("done");
    assert_eq!(answer["result"]["isError"], true);
    assert!(answer["result"]["content"][0]["text"].as_str().unwrap().contains("Kiko is not running"));
    thread::sleep(Duration::from_millis(1200));
    assert!(channel.still_running());

    // Kiko starts: the channel finds it on its own and the chat works.
    let kiko = FakeKiko::start(port);
    kiko.expect(|seen| matches!(seen, Seen::Register(_)).then_some(()));
    kiko.outbox.send(ChatMessage { id: "m2".into(), text: "hello".into(), at: 2 }).unwrap();
    let event = channel.next_matching(|m| m["method"] == "notifications/claude/channel");
    assert_eq!(event["params"]["content"], "hello");
    assert_eq!(channel.reply("hi")["result"]["isError"], false);
}

#[test]
fn a_missing_config_is_not_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let mut channel = Channel::spawn(Some(&dir.path().join("nope.json")), Some(SESSION));
    channel.handshake();
    assert_eq!(channel.reply("done")["result"]["isError"], true);
    assert!(channel.still_running());
}

#[test]
fn a_wrong_token_gets_no_messages() {
    let dir = tempfile::tempdir().unwrap();
    let (kiko, port) = FakeKiko::start_any();
    let config = dir.path().join("config.json");
    std::fs::write(&config, json!({ "port": port, "token": "wrong" }).to_string()).unwrap();
    let mut channel = Channel::spawn(Some(&config), Some(SESSION));
    kiko.expect(|seen| matches!(seen, Seen::BadToken).then_some(()));
    channel.handshake();
    assert!(channel.still_running());
}

#[test]
fn without_a_session_it_never_contacts_kiko() {
    // e.g. a `claude mcp get` health check, or someone running it by hand.
    let dir = tempfile::tempdir().unwrap();
    let (kiko, port) = FakeKiko::start_any();
    let mut channel = Channel::spawn(Some(&write_config(dir.path(), port)), None);
    channel.handshake();
    let answer = channel.reply("done");
    assert_eq!(answer["result"]["isError"], true);
    assert!(kiko.seen.recv_timeout(Duration::from_millis(500)).is_err());
}

#[test]
fn closing_stdin_unregisters_and_exits() {
    let dir = tempfile::tempdir().unwrap();
    let (kiko, port) = FakeKiko::start_any();
    let mut channel = Channel::spawn(Some(&write_config(dir.path(), port)), Some(SESSION));
    kiko.expect(|seen| matches!(seen, Seen::Register(_)).then_some(()));
    channel.handshake();

    channel.stdin = None; // Claude Code closes the pipe when the session ends
    kiko.expect(|seen| matches!(seen, Seen::Unregister).then_some(()));
    let deadline = Instant::now() + WAIT;
    while channel.still_running() {
        assert!(Instant::now() < deadline, "kiko-channel kept running after stdin closed");
        thread::sleep(Duration::from_millis(20));
    }
    assert!(channel.child.wait().unwrap().success());
}
