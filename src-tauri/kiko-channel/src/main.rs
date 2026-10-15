//! `kiko-channel --config <Kiko's config.json>`, started by Claude Code (see lib.rs).

use std::{
    collections::hash_map::RandomState,
    env,
    hash::{BuildHasher, Hasher},
    io::{self, BufRead, Write},
    path::PathBuf,
    process,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use kiko_channel::{
    bridge::Bridge,
    mcp::{McpServer, channel_notification},
    platform,
    protocol::{ChatMessage, Register},
};
use serde_json::Value;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// stdout, shared by the answers to Claude Code's requests and the channel events.
struct Output {
    stdout: Mutex<io::Stdout>,
    initialized: Mutex<bool>,
    ready: Condvar,
}

impl Output {
    /// One JSON message per line. If Claude Code is gone, so is the reason to run.
    fn write(&self, message: &Value) {
        let mut stdout = lock(&self.stdout);
        if writeln!(stdout, "{message}")
            .and_then(|()| stdout.flush())
            .is_err()
        {
            process::exit(0);
        }
    }

    fn set_initialized(&self) {
        *lock(&self.initialized) = true;
        self.ready.notify_all();
    }

    /// Channel events wait for the end of the MCP handshake.
    fn push(&self, messages: Vec<ChatMessage>) {
        let mut initialized = lock(&self.initialized);
        while !*initialized {
            initialized = self
                .ready
                .wait(initialized)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        drop(initialized);
        for message in &messages {
            self.write(&channel_notification(message));
        }
    }
}

/// 128 random bits as hex, from the standard library's per-process random hasher keys.
fn new_channel_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (0..2)
        .map(|round: u64| {
            let mut hasher = RandomState::new().build_hasher();
            hasher.write_u128(nanos);
            hasher.write_u32(process::id());
            hasher.write_u64(round);
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--version") {
        println!("kiko-channel {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let config = args
        .iter()
        .position(|arg| arg == "--config")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let cwd = env::var("CLAUDE_PROJECT_DIR")
        .ok()
        .filter(|dir| !dir.is_empty())
        .or_else(|| env::current_dir().ok().map(|d| d.display().to_string()))
        .unwrap_or_default();
    let bridge = Arc::new(Bridge::new(
        config,
        Register {
            channel: new_channel_id(),
            session_id: env::var("CLAUDE_CODE_SESSION_ID").unwrap_or_default(),
            cwd,
            pid: process::id(),
            parent_pid: platform::parent_pid(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
    ));
    let output = Arc::new(Output {
        stdout: Mutex::new(io::stdout()),
        initialized: Mutex::new(false),
        ready: Condvar::new(),
    });

    if bridge.has_session() {
        let (bridge, output) = (Arc::clone(&bridge), Arc::clone(&output));
        // If the thread can't start, the MCP side still works; replies report Kiko unreachable.
        let _ = thread::Builder::new()
            .name("kiko-bridge".into())
            .spawn(move || bridge.run(|messages| output.push(messages)));
    }

    let mut server = McpServer::new(|text: &str| bridge.reply(text));
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(answer) = server.handle_line(&line) {
            output.write(&answer);
        }
        if server.initialized() {
            output.set_initialized();
        }
    }
    // Claude Code closed stdin: the session is over.
    bridge.unregister();
}
