//! Keeps the channel connected to Kiko: registers the session, waits for the user's messages and
//! hands them to the MCP side, and carries Claude's replies back. Kiko may be closed, start later or
//! restart at any time: every failure is retried quietly in the background, and the session never
//! waits for Kiko.

use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard},
    thread,
    time::Duration,
};

use crate::{
    kiko::{Kiko, KikoError},
    protocol::{ChannelRef, ChatMessage, Endpoint, Inbox, POLL_TIMEOUT, Register, Reply},
};

/// Limit on quick requests (register, reply, unregister). Kiko answers them at once.
const QUICK: Duration = Duration::from_secs(3);
/// Kiko answers a poll within POLL_TIMEOUT; this leaves room for a slow machine.
const POLL_LIMIT: Duration = Duration::from_secs(POLL_TIMEOUT.as_secs() + 15);
const FIRST_RETRY: Duration = Duration::from_millis(500);
const LAST_RETRY: Duration = Duration::from_secs(5);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct Bridge {
    /// Kiko's config.json (port and token). Re-read on every reconnection: the token may change.
    config: Option<PathBuf>,
    register: Register,
    kiko: Mutex<Option<Kiko>>,
}

impl Bridge {
    pub fn new(config: Option<PathBuf>, register: Register) -> Self {
        Self {
            config,
            register,
            kiko: Mutex::new(None),
        }
    }

    /// Started by a Claude Code session (not by hand, nor by a `claude mcp` health check), with a
    /// config to find Kiko.
    pub fn has_session(&self) -> bool {
        !self.register.session_id.is_empty() && self.config.is_some()
    }

    fn me(&self) -> ChannelRef {
        ChannelRef {
            channel: self.register.channel.clone(),
        }
    }

    /// Kiko as last read from its config, or read now.
    fn kiko(&self, reload: bool) -> Option<Kiko> {
        let mut current = lock(&self.kiko);
        if reload || current.is_none() {
            *current = self.config.as_deref().and_then(|path| Kiko::load(path).ok());
        }
        current.clone()
    }

    fn register_with(&self, kiko: &Kiko) -> Result<(), KikoError> {
        kiko.post(Endpoint::Register, &self.register, QUICK).map(drop)
    }

    /// Runs forever: registers, then polls, passing each batch of messages to `deliver`.
    pub fn run(&self, mut deliver: impl FnMut(Vec<ChatMessage>)) {
        let mut retry = FIRST_RETRY;
        let mut registered = false;
        let mut reachable = true;
        loop {
            let outcome = match self.kiko(!registered) {
                None => Err(None),
                Some(kiko) if !registered => self
                    .register_with(&kiko)
                    .map(|()| {
                        registered = true;
                        eprintln!("kiko-channel: connected to Kiko on port {}", kiko.port());
                    })
                    .map_err(Some),
                Some(kiko) => kiko
                    .post(Endpoint::Poll, &self.me(), POLL_LIMIT)
                    .map(|body| {
                        // A body that isn't an inbox is skipped: the next poll goes on as usual.
                        if let Ok(inbox) = serde_json::from_slice::<Inbox>(&body)
                            && !inbox.messages.is_empty()
                        {
                            deliver(inbox.messages);
                        }
                    })
                    .map_err(Some),
            };
            match outcome {
                Ok(()) => {
                    retry = FIRST_RETRY;
                    reachable = true;
                }
                // Kiko restarted and doesn't know this channel any more: register again at once.
                Err(Some(KikoError::Status(404))) if registered => registered = false,
                Err(error) => {
                    if reachable {
                        let why = error.map_or("Kiko's config is missing".into(), |e| e.to_string());
                        eprintln!("kiko-channel: {why}; retrying quietly in the background");
                        reachable = false;
                    }
                    registered = false;
                    thread::sleep(retry);
                    retry = (retry * 2).min(LAST_RETRY);
                }
            }
        }
    }

    /// Sends Claude's reply to Kiko. Registers first if Kiko has just (re)started.
    pub fn reply(&self, text: &str) -> Result<(), String> {
        if self.register.session_id.is_empty() {
            return Err("this channel was not started by a Claude Code session".into());
        }
        let kiko = self.kiko(false).ok_or("Kiko's config is missing")?;
        let reply = Reply {
            channel: self.register.channel.clone(),
            text: text.to_string(),
        };
        match kiko.post(Endpoint::Reply, &reply, QUICK) {
            Err(KikoError::Status(404)) => {
                self.register_with(&kiko).map_err(|e| e.to_string())?;
                kiko.post(Endpoint::Reply, &reply, QUICK)
            }
            // The token may have changed since the config was read.
            Err(KikoError::Status(401)) => {
                let kiko = self.kiko(true).ok_or("Kiko's config is missing")?;
                kiko.post(Endpoint::Reply, &reply, QUICK)
            }
            other => other,
        }
        .map(drop)
        .map_err(|e| e.to_string())
    }

    /// Tells Kiko the session closed the channel. Best effort: Kiko also notices on its own.
    pub fn unregister(&self) {
        if !self.has_session() {
            return;
        }
        if let Some(kiko) = self.kiko(false) {
            let _ = kiko.post(Endpoint::Unregister, &self.me(), Duration::from_secs(1));
        }
    }
}
