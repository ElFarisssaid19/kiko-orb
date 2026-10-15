//! End to end: the exact command the installer writes, run through `sh -c` with the event JSON on
//! stdin (as Claude Code runs command hooks), with real curl, against the hook server.

use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::Path,
    process::{Command, Stdio},
    sync::{Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use serde_json::{Map, Value, json};

use kiko_channel::{
    kiko::Kiko,
    protocol::{Endpoint, Register},
};

use super::{
    events::AgentEvent,
    install::{TOKEN_HEADER, hook_command, shell_path},
    parse::{Hook, PermissionDecision},
    server::{Emitted, HookServer, UserResponse},
};

const TOKEN: &str = "curl-test-token-0123456789abcdef0123456789abcdef";

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Runs a hook command like Claude Code does: shell form (`sh -c`), the event JSON on stdin.
fn run_hook(command: &str, input: &Value) -> Run {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    Run {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

/// The header file as Kiko writes it, and how the hook commands refer to it.
fn header_file(dir: &Path, token: &str) -> String {
    let path = dir.join("auth-header");
    fs::write(&path, format!("{TOKEN_HEADER}: {token}\n")).unwrap();
    shell_path(&path, Path::new("/nonexistent-home"))
}

fn start_server() -> (HookServer, mpsc::Receiver<AgentEvent>) {
    let (sender, receiver) = mpsc::channel();
    let sender = Mutex::new(sender);
    let server = HookServer::start(0, TOKEN.into(), Duration::from_secs(60), move |emitted| {
        if let Emitted::Event(event) = emitted {
            let _ = sender.lock().unwrap().send(event);
        }
    })
    .unwrap();
    (server, receiver)
}

fn wait_for_request(server: &HookServer) -> String {
    for _ in 0..300 {
        let id = server
            .pending_events()
            .into_iter()
            .find_map(|event| match event {
                AgentEvent::PermissionRequested { request, .. } => Some(request.id),
                AgentEvent::QuestionAsked { request, .. } => Some(request.id),
                _ => None,
            });
        if let Some(id) = id {
            return id;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("the request never reached Kiko");
}

/// PermissionRequest input from the hooks reference.
fn permission_input() -> Value {
    json!({
      "session_id": "abc123",
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

/// Runs a held hook in the background, answers it in Kiko, and returns what the command printed.
fn answered(server: &HookServer, command: String, input: Value, response: UserResponse) -> Run {
    let hook = thread::spawn(move || run_hook(&command, &input));
    let id = wait_for_request(server);
    server.respond(&id, response).unwrap();
    hook.join().unwrap()
}

#[test]
fn an_observed_event_reaches_kiko_and_prints_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (server, events) = start_server();
    let command = hook_command(
        server.port(),
        &header_file(dir.path(), TOKEN),
        Hook::PreToolUse,
        2,
    );
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
    let run = run_hook(&command, &input);
    assert_eq!(
        (run.code, run.stdout.as_str(), run.stderr.as_str()),
        (Some(0), "", "")
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
fn permission_decisions_come_back_on_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _events) = start_server();
    let command = hook_command(
        server.port(),
        &header_file(dir.path(), TOKEN),
        Hook::PermissionRequest,
        115,
    );

    let always = answered(
        &server,
        command.clone(),
        permission_input(),
        UserResponse::Permission {
            decision: PermissionDecision::Always,
        },
    );
    assert_eq!(always.code, Some(0));
    let output: Value = serde_json::from_str(&always.stdout).unwrap();
    assert_eq!(
        output,
        json!({
          "hookSpecificOutput": {
            "hookEventName": "PermissionRequest",
            "decision": {
              "behavior": "allow",
              "updatedPermissions": [{
                "type": "addRules",
                "rules": [{ "toolName": "Bash", "ruleContent": "rm -rf node_modules" }],
                "behavior": "allow",
                "destination": "localSettings"
              }]
            }
          }
        })
    );

    let deny = answered(
        &server,
        command.clone(),
        permission_input(),
        UserResponse::Permission {
            decision: PermissionDecision::Deny,
        },
    );
    let output: Value = serde_json::from_str(&deny.stdout).unwrap();
    assert_eq!(output["hookSpecificOutput"]["decision"]["behavior"], "deny");

    // "Reply in terminal": no output, so Claude Code shows its own prompt.
    let released = answered(&server, command, permission_input(), UserResponse::Release);
    assert_eq!((released.code, released.stdout.as_str()), (Some(0), ""));
}

#[test]
fn a_question_answer_comes_back_on_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _events) = start_server();
    let command = hook_command(
        server.port(),
        &header_file(dir.path(), TOKEN),
        Hook::PreToolUse,
        115,
    );
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
    let mut answers = Map::new();
    answers.insert("Which framework?".into(), json!("React"));
    let run = answered(&server, command, input, UserResponse::Question { answers });
    assert_eq!(run.code, Some(0));
    let output: Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(output["hookSpecificOutput"]["permissionDecision"], "allow");
    assert_eq!(
        output["hookSpecificOutput"]["updatedInput"]["answers"]["Which framework?"],
        "React"
    );
}

#[test]
fn with_kiko_closed_the_hook_exits_0_silently_and_at_once() {
    let dir = tempfile::tempdir().unwrap();
    // A port nothing listens on any more.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    for (hook, max_time) in [(Hook::PreToolUse, 2), (Hook::PermissionRequest, 115)] {
        let command = hook_command(port, &header_file(dir.path(), TOKEN), hook, max_time);
        let started = Instant::now();
        let run = run_hook(&command, &permission_input());
        assert_eq!(
            (run.code, run.stdout.as_str(), run.stderr.as_str()),
            (Some(0), "", "")
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "a refused connection fails at once"
        );
    }
}

#[test]
fn a_wrong_token_is_refused_silently() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _events) = start_server();
    let command = hook_command(
        server.port(),
        &header_file(dir.path(), "not-the-token"),
        Hook::PermissionRequest,
        115,
    );
    let run = run_hook(&command, &permission_input());
    assert_eq!(
        (run.code, run.stdout.as_str(), run.stderr.as_str()),
        (Some(0), "", "")
    );
    assert!(
        server.pending_events().is_empty(),
        "nothing reached the island"
    );
}

#[test]
fn the_hook_command_sends_the_claude_code_process_id() {
    let dir = tempfile::tempdir().unwrap();
    let (server, events) = start_server();
    let command = hook_command(
        server.port(),
        &header_file(dir.path(), TOKEN),
        Hook::SessionStart,
        2,
    );
    // Claude Code runs `sh -c <command>`; here this test process plays Claude Code, so the hook's
    // `$PPID` is this process.
    let input = json!({ "session_id": "abc123", "cwd": "/home/user/my-project", "hook_event_name": "SessionStart", "source": "clear" });
    assert_eq!(run_hook(&command, &input).code, Some(0));

    // A channel started by the same process, with a stale CLAUDE_CODE_SESSION_ID: the pid wins.
    let register = Register {
        channel: "c1".into(),
        session_id: "stale-id-from-before-clear".into(),
        cwd: "/home/user/my-project".into(),
        pid: 1,
        parent_pid: std::process::id(),
        version: "test".into(),
    };
    Kiko::new(server.port(), TOKEN)
        .post(Endpoint::Register, &register, Duration::from_secs(2))
        .unwrap();
    let linked = events
        .iter()
        .find_map(|event| match event {
            AgentEvent::ChatLinked { session_id, .. } => Some(session_id),
            _ => None,
        })
        .unwrap();
    assert_eq!(linked, "abc123");
}
