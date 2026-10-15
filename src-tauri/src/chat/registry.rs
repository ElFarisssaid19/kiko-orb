//! Which kiko-channel serves which session, the messages waiting for each channel, and the polls Kiko
//! holds open for them.
//!
//! A channel is linked to a session through its parent process: the Claude Code process that started
//! it also runs Kiko's hooks, and every hook says which session that process runs now (the hook
//! commands send `$PPID`). So the link follows `/clear` and `/resume`, where the process stays and the
//! session changes. The session id the channel registered with (`CLAUDE_CODE_SESSION_ID`, frozen when
//! Claude Code started it) is only a fallback until a hook from that process arrives.
//!
//! Pure state: the caller holds the lock, then answers the returned polls and emits the returned events.

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    time::{Duration, Instant},
};

use kiko_channel::protocol::{ChatMessage, Inbox, POLL_TIMEOUT, Register};

use crate::hooks::{AgentEvent, Hook};

/// Longest message the user can send, and longest reply Kiko accepts.
pub const MAX_TEXT_BYTES: usize = 256 * 1024;
const MAX_ID_LEN: usize = 64;
/// Sessions that ended recently: a late event from one of them must not link its process again.
const ENDED_KEPT: usize = 64;
/// Claude Code processes remembered; every session's hooks report one, with a chat channel or not.
const PROCESSES_KEPT: usize = 256;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// A held poll is answered empty after this long; the channel polls again at once.
    pub poll_timeout: Duration,
    /// A channel that hasn't polled for this long is gone (killed before it could unregister).
    pub stale_after: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            poll_timeout: POLL_TIMEOUT,
            stale_after: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChatError {
    /// Kiko doesn't know this channel (it restarted, or the channel went stale): HTTP 404.
    UnknownChannel,
    /// The channel's Claude Code process runs no session Kiko knows of right now: HTTP 409.
    NoSession,
    /// The session has no channel: it wasn't started with Kiko's chat, or its channel is gone.
    NoChannel,
    Invalid(&'static str),
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatError::UnknownChannel => write!(f, "Kiko doesn't know this channel."),
            ChatError::NoSession => write!(f, "This channel has no session right now."),
            ChatError::NoChannel => write!(
                f,
                "This session has no chat channel. Start Claude Code with Kiko's launch command to chat."
            ),
            ChatError::Invalid(why) => write!(f, "{why}"),
        }
    }
}

/// Polls to answer and events to emit after a change.
#[derive(Debug)]
pub struct Effects<W> {
    pub answers: Vec<(W, Inbox)>,
    pub events: Vec<AgentEvent>,
}

impl<W> Effects<W> {
    fn events(events: Vec<AgentEvent>) -> Self {
        Self {
            answers: Vec::new(),
            events,
        }
    }
}

struct Channel<W> {
    register: Register,
    /// Messages not picked up yet, each with the session it was written to.
    inbox: Vec<(String, ChatMessage)>,
    /// The poll held open until a message arrives or `poll_timeout` passes, and since when.
    poll: Option<(W, Instant)>,
    last_seen: Instant,
}

impl<W> Channel<W> {
    fn id(&self) -> &str {
        &self.register.channel
    }

    /// The waiting messages written to `session`; the others were written to an earlier session of
    /// this process and are dropped.
    fn take_inbox(&mut self, session: Option<&str>) -> Vec<ChatMessage> {
        self.inbox
            .drain(..)
            .filter(|(to, _)| Some(to.as_str()) == session)
            .map(|(_, message)| message)
            .collect()
    }
}

/// A Claude Code process and the session it runs now (`None`: that session just ended).
struct Process {
    session: Option<String>,
    /// For forgetting the least recently heard processes first.
    heard: u64,
}

pub struct Registry<W> {
    limits: Limits,
    /// In registration order: when two channels end up on the same session, the newest serves it.
    channels: Vec<Channel<W>>,
    processes: HashMap<u32, Process>,
    heard: u64,
    ended: VecDeque<String>,
    /// Session id → id of the channel serving it, as last announced to the frontend.
    links: BTreeMap<String, String>,
    replies: u64,
}

fn validate_text(text: &str) -> Result<(), ChatError> {
    if text.trim().is_empty() {
        return Err(ChatError::Invalid("The message is empty."));
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(ChatError::Invalid("The message is too long."));
    }
    Ok(())
}

impl<W> Registry<W> {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            channels: Vec::new(),
            processes: HashMap::new(),
            heard: 0,
            ended: VecDeque::new(),
            links: BTreeMap::new(),
            replies: 0,
        }
    }

    /// A hook event from session `session_id`, run by Claude Code process `pid` (`None` for the hooks
    /// of an earlier Kiko, which don't send it).
    pub fn observe(
        &mut self,
        pid: Option<u32>,
        hook: Hook,
        session_id: &str,
        at: u64,
    ) -> Vec<AgentEvent> {
        if session_id.is_empty() {
            return Vec::new();
        }
        if hook == Hook::SessionEnd {
            if !self.ended.iter().any(|s| s == session_id) {
                if self.ended.len() == ENDED_KEPT {
                    self.ended.pop_front();
                }
                self.ended.push_back(session_id.to_string());
            }
            // Only if that process still runs this session: a late SessionEnd must not undo a newer start.
            if let Some(pid) = pid
                && let Some(process) = self.processes.get_mut(&pid)
                && process.session.as_deref() == Some(session_id)
            {
                process.session = None;
            }
        } else {
            if hook == Hook::SessionStart {
                // `--resume` brings back a session that ended in an earlier process.
                self.ended.retain(|s| s != session_id);
            } else if self.ended.iter().any(|s| s == session_id) {
                // A late event from a session that already ended.
                return Vec::new();
            }
            if let Some(pid) = pid {
                self.heard += 1;
                self.processes.insert(
                    pid,
                    Process {
                        session: Some(session_id.to_string()),
                        heard: self.heard,
                    },
                );
            }
        }
        self.forget_old_processes();
        self.relink(at)
    }

    /// A channel started (or registers again after Kiko restarted, or after a 404).
    pub fn register(&mut self, register: Register, now: Instant, at: u64) -> Effects<W> {
        match self
            .channels
            .iter_mut()
            .find(|c| c.register.channel == register.channel)
        {
            Some(channel) => {
                channel.register = register;
                channel.last_seen = now;
            }
            None => self.channels.push(Channel {
                register,
                inbox: Vec::new(),
                poll: None,
                last_seen: now,
            }),
        }
        Effects::events(self.relink(at))
    }

    /// The channel's session closed it.
    pub fn unregister(&mut self, channel: &str, at: u64) -> Result<Effects<W>, ChatError> {
        let index = self.index(channel).ok_or(ChatError::UnknownChannel)?;
        let removed = self.channels.remove(index);
        Ok(self.removed(vec![removed], at))
    }

    /// The channel waits for messages. Gives the waiter back when the channel is unknown (404): the
    /// channel then registers again.
    pub fn poll(
        &mut self,
        channel: &str,
        waiter: W,
        now: Instant,
        at: u64,
    ) -> Result<Effects<W>, W> {
        let Some(index) = self.index(channel) else {
            return Err(waiter);
        };
        let session = self.session_of(index).map(str::to_string);
        let channel = &mut self.channels[index];
        channel.last_seen = now;
        let mut effects = Effects::events(Vec::new());
        // A channel polls once at a time; an older poll still held is answered empty.
        if let Some((older, _)) = channel.poll.take() {
            effects.answers.push((older, Inbox::default()));
        }
        let messages = channel.take_inbox(session.as_deref());
        match (session, messages.is_empty()) {
            (Some(session_id), false) => {
                effects.events.push(AgentEvent::ChatDelivered {
                    at,
                    session_id,
                    message_ids: messages.iter().map(|m| m.id.clone()).collect(),
                });
                effects.answers.push((waiter, Inbox { messages }));
            }
            _ => channel.poll = Some((waiter, now)),
        }
        Ok(effects)
    }

    /// The user's message for `session_id`: handed to the poll its channel holds, or kept for its next one.
    pub fn send(
        &mut self,
        session_id: &str,
        message: ChatMessage,
        at: u64,
    ) -> Result<Effects<W>, ChatError> {
        validate_text(&message.text)?;
        if message.id.is_empty() || message.id.len() > MAX_ID_LEN {
            return Err(ChatError::Invalid("The message id is not valid."));
        }
        let channel = self.links.get(session_id).ok_or(ChatError::NoChannel)?;
        let index = self.index(channel).ok_or(ChatError::NoChannel)?;
        let channel = &mut self.channels[index];
        let mut effects = Effects::events(Vec::new());
        match channel.poll.take() {
            Some((waiter, _)) => {
                let mut messages = channel.take_inbox(Some(session_id));
                messages.push(message);
                effects.events.push(AgentEvent::ChatDelivered {
                    at,
                    session_id: session_id.to_string(),
                    message_ids: messages.iter().map(|m| m.id.clone()).collect(),
                });
                effects.answers.push((waiter, Inbox { messages }));
            }
            None => channel.inbox.push((session_id.to_string(), message)),
        }
        Ok(effects)
    }

    /// Claude's answer, from the channel's reply tool: an event for the channel's current session.
    pub fn reply(
        &mut self,
        channel: &str,
        text: &str,
        now: Instant,
        at: u64,
    ) -> Result<AgentEvent, ChatError> {
        let index = self.index(channel).ok_or(ChatError::UnknownChannel)?;
        self.channels[index].last_seen = now;
        validate_text(text)?;
        let session_id = self
            .session_of(index)
            .ok_or(ChatError::NoSession)?
            .to_string();
        self.replies += 1;
        Ok(AgentEvent::ChatReply {
            at,
            session_id,
            message: ChatMessage {
                id: format!("reply-{}", self.replies),
                text: text.to_string(),
                at,
            },
        })
    }

    /// Answers the polls held for `poll_timeout`, and forgets the channels that stopped polling.
    pub fn tick(&mut self, now: Instant, at: u64) -> Effects<W> {
        let mut answers = Vec::new();
        for channel in &mut self.channels {
            let expired = channel
                .poll
                .as_ref()
                .is_some_and(|(_, since)| now.duration_since(*since) >= self.limits.poll_timeout);
            if expired && let Some((waiter, _)) = channel.poll.take() {
                answers.push((waiter, Inbox::default()));
                channel.last_seen = now;
            }
        }
        let stale_after = self.limits.stale_after;
        let (stale, live): (Vec<_>, Vec<_>) = self.channels.drain(..).partition(|channel| {
            channel.poll.is_none() && now.duration_since(channel.last_seen) >= stale_after
        });
        self.channels = live;
        let mut effects = if stale.is_empty() {
            Effects::events(Vec::new())
        } else {
            self.removed(stale, at)
        };
        effects.answers.extend(answers);
        effects
    }

    /// Sessions a channel serves now, for a window that just opened.
    pub fn linked_sessions(&self) -> Vec<String> {
        self.links.keys().cloned().collect()
    }

    fn index(&self, channel: &str) -> Option<usize> {
        self.channels.iter().position(|c| c.id() == channel)
    }

    /// The session the channel at `index` serves: what its Claude Code process runs now, else (no hook
    /// heard from that process yet) the session it registered with, unless that one ended.
    fn session_of(&self, index: usize) -> Option<&str> {
        let register = &self.channels[index].register;
        if register.parent_pid != 0
            && let Some(process) = self.processes.get(&register.parent_pid)
        {
            return process.session.as_deref();
        }
        let fallback = register.session_id.as_str();
        (!fallback.is_empty() && !self.ended.iter().any(|s| s == fallback)).then_some(fallback)
    }

    /// Recomputes which channel serves which session; events for the sessions that gained or lost one.
    fn relink(&mut self, at: u64) -> Vec<AgentEvent> {
        let mut links = BTreeMap::new();
        for index in 0..self.channels.len() {
            if let Some(session) = self.session_of(index) {
                links.insert(session.to_string(), self.channels[index].id().to_string());
            }
        }
        let mut events: Vec<AgentEvent> = self
            .links
            .keys()
            .filter(|session| !links.contains_key(*session))
            .map(|session_id| AgentEvent::ChatUnlinked {
                at,
                session_id: session_id.clone(),
            })
            .collect();
        events.extend(
            links
                .keys()
                .filter(|session| !self.links.contains_key(*session))
                .map(|session_id| AgentEvent::ChatLinked {
                    at,
                    session_id: session_id.clone(),
                }),
        );
        self.links = links;
        events
    }

    /// After channels went away: their held polls are answered, their sessions lose the link.
    fn removed(&mut self, channels: Vec<Channel<W>>, at: u64) -> Effects<W> {
        let answers = channels
            .into_iter()
            .filter_map(|channel| channel.poll)
            .map(|(waiter, _)| (waiter, Inbox::default()))
            .collect();
        self.forget_old_processes();
        Effects {
            answers,
            events: self.relink(at),
        }
    }

    /// Keeps at most PROCESSES_KEPT processes, dropping the least recently heard ones that no channel
    /// needs, and those whose session ended and have no channel.
    fn forget_old_processes(&mut self) {
        let channels: Vec<u32> = self
            .channels
            .iter()
            .map(|c| c.register.parent_pid)
            .collect();
        self.processes
            .retain(|pid, process| process.session.is_some() || channels.contains(pid));
        while self.processes.len() > PROCESSES_KEPT {
            let oldest = self
                .processes
                .iter()
                .filter(|(pid, _)| !channels.contains(pid))
                .min_by_key(|(_, process)| process.heard)
                .map(|(pid, _)| *pid);
            match oldest {
                Some(pid) => self.processes.remove(&pid),
                None => break,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: u64 = 1_000;

    fn limits() -> Limits {
        Limits {
            poll_timeout: Duration::from_secs(25),
            stale_after: Duration::from_secs(10),
        }
    }

    /// What kiko-channel registers: started by Claude Code process `parent_pid`, with the session id
    /// Claude Code gave it then.
    fn register(channel: &str, parent_pid: u32, session_id: &str) -> Register {
        Register {
            channel: channel.into(),
            session_id: session_id.into(),
            cwd: "/home/user/my-project".into(),
            pid: parent_pid + 1,
            parent_pid,
            version: "0.1.0".into(),
        }
    }

    fn message(id: &str, text: &str) -> ChatMessage {
        ChatMessage {
            id: id.into(),
            text: text.into(),
            at: AT,
        }
    }

    fn linked(session: &str) -> AgentEvent {
        AgentEvent::ChatLinked {
            at: AT,
            session_id: session.into(),
        }
    }

    fn unlinked(session: &str) -> AgentEvent {
        AgentEvent::ChatUnlinked {
            at: AT,
            session_id: session.into(),
        }
    }

    /// The waiter stands for a held HTTP request; the tests name them.
    type Test = Registry<&'static str>;

    #[test]
    fn a_channel_follows_its_process_through_clear() {
        let now = Instant::now();
        let mut chat = Test::new(limits());
        assert!(
            chat.observe(Some(4242), Hook::SessionStart, "abc123", AT)
                .is_empty()
        );
        assert_eq!(
            chat.register(register("c1", 4242, "abc123"), now, AT)
                .events,
            [linked("abc123")]
        );

        // /clear: the same process ends abc123 and starts def456. The channel keeps running, with
        // CLAUDE_CODE_SESSION_ID still abc123 in its environment.
        assert_eq!(
            chat.observe(Some(4242), Hook::SessionEnd, "abc123", AT),
            [unlinked("abc123")]
        );
        assert_eq!(
            chat.observe(Some(4242), Hook::SessionStart, "def456", AT),
            [linked("def456")]
        );
        assert_eq!(
            chat.send("abc123", message("m1", "hi"), AT).unwrap_err(),
            ChatError::NoChannel
        );
        chat.send("def456", message("m2", "hello"), AT).unwrap();
        let effects = chat.poll("c1", "poll-1", now, AT).unwrap();
        assert_eq!(effects.answers[0].1.messages, [message("m2", "hello")]);
        let reply = chat.reply("c1", "Hi!", now, AT).unwrap();
        assert!(
            matches!(reply, AgentEvent::ChatReply { session_id, .. } if session_id == "def456")
        );
    }

    #[test]
    fn two_sessions_in_two_processes_each_get_their_own_channel() {
        let now = Instant::now();
        let mut chat = Test::new(limits());
        chat.observe(Some(100), Hook::SessionStart, "one", AT);
        chat.observe(Some(200), Hook::SessionStart, "two", AT);
        chat.register(register("c1", 100, "one"), now, AT);
        chat.register(register("c2", 200, "two"), now, AT);
        assert_eq!(chat.linked_sessions(), ["one", "two"]);

        chat.poll("c1", "poll-1", now, AT).unwrap();
        chat.poll("c2", "poll-2", now, AT).unwrap();
        let to_two = chat.send("two", message("m1", "for two"), AT).unwrap();
        assert_eq!(to_two.answers.len(), 1);
        assert_eq!(to_two.answers[0].0, "poll-2");
        let to_one = chat.send("one", message("m2", "for one"), AT).unwrap();
        assert_eq!(to_one.answers[0].0, "poll-1");
        assert_eq!(
            to_one.events,
            [AgentEvent::ChatDelivered {
                at: AT,
                session_id: "one".into(),
                message_ids: vec!["m2".into()]
            }]
        );

        // One Claude Code exits: only its session loses the chat.
        chat.observe(Some(100), Hook::SessionEnd, "one", AT);
        let effects = chat.unregister("c1", AT).unwrap();
        assert!(effects.events.is_empty(), "already unlinked at SessionEnd");
        assert_eq!(chat.linked_sessions(), ["two"]);
    }

    #[test]
    fn the_registered_session_id_is_only_a_fallback() {
        let now = Instant::now();
        let mut chat = Test::new(limits());
        // Kiko started mid-session: no hook heard from process 4242 yet.
        assert_eq!(
            chat.register(register("c1", 4242, "abc123"), now, AT)
                .events,
            [linked("abc123")]
        );
        // Its first hook tells the truth: the process runs def456 now (a /clear happened before).
        assert_eq!(
            chat.observe(Some(4242), Hook::PreToolUse, "def456", AT),
            [unlinked("abc123"), linked("def456")]
        );
        // Hooks without the process id (an earlier Kiko's) can't move it.
        assert!(
            chat.observe(None, Hook::SessionStart, "ghi789", AT)
                .is_empty()
        );
        assert_eq!(chat.linked_sessions(), ["def456"]);
    }

    #[test]
    fn without_process_ids_the_fallback_ends_with_its_session() {
        let now = Instant::now();
        let mut chat = Test::new(limits());
        chat.register(register("c1", 0, "abc123"), now, AT);
        assert_eq!(chat.linked_sessions(), ["abc123"]);
        assert_eq!(
            chat.observe(None, Hook::SessionEnd, "abc123", AT),
            [unlinked("abc123")]
        );
        assert!(
            chat.observe(None, Hook::SessionStart, "def456", AT)
                .is_empty()
        );
    }

    #[test]
    fn a_late_event_from_an_ended_session_changes_nothing() {
        let now = Instant::now();
        let mut chat = Test::new(limits());
        chat.observe(Some(4242), Hook::SessionStart, "abc123", AT);
        chat.register(register("c1", 4242, "abc123"), now, AT);
        chat.observe(Some(4242), Hook::SessionEnd, "abc123", AT);
        chat.observe(Some(4242), Hook::SessionStart, "def456", AT);
        assert!(
            chat.observe(Some(4242), Hook::PostToolUse, "abc123", AT)
                .is_empty()
        );
        assert!(
            chat.observe(Some(4242), Hook::SessionEnd, "abc123", AT)
                .is_empty()
        );
        assert_eq!(chat.linked_sessions(), ["def456"]);
        // `/resume abc123` in the same process brings it back.
        assert_eq!(
            chat.observe(Some(4242), Hook::SessionStart, "abc123", AT),
            [unlinked("def456"), linked("abc123")]
        );
    }

    #[test]
    fn a_message_waits_for_the_next_poll_and_a_cleared_one_is_dropped() {
        let now = Instant::now();
        let mut chat = Test::new(limits());
        chat.observe(Some(4242), Hook::SessionStart, "abc123", AT);
        chat.register(register("c1", 4242, "abc123"), now, AT);
        let queued = chat.send("abc123", message("m1", "first"), AT).unwrap();
        assert!(queued.answers.is_empty() && queued.events.is_empty());
        let effects = chat.poll("c1", "poll-1", now, AT).unwrap();
        assert_eq!(
            effects.answers,
            [(
                "poll-1",
                Inbox {
                    messages: vec![message("m1", "first")]
                }
            )]
        );

        // The next poll times out empty; a message arrives while the channel is between two polls.
        chat.poll("c1", "poll-2", now, AT).unwrap();
        let later = now + Duration::from_secs(25);
        let effects = chat.tick(later, AT);
        assert_eq!(effects.answers, [("poll-2", Inbox::default())]);
        chat.send("abc123", message("m2", "late"), AT).unwrap();
        // Written to abc123, but the process moved on (/clear) before its channel picked it up.
        chat.observe(Some(4242), Hook::SessionEnd, "abc123", AT);
        chat.observe(Some(4242), Hook::SessionStart, "def456", AT);
        let effects = chat.poll("c1", "poll-3", later, AT).unwrap();
        assert!(
            effects.answers.is_empty() && effects.events.is_empty(),
            "m2 is dropped, the poll is held"
        );
    }

    #[test]
    fn polls_time_out_and_silent_channels_go_stale() {
        let start = Instant::now();
        let mut chat = Test::new(limits());
        chat.observe(Some(4242), Hook::SessionStart, "abc123", AT);
        chat.register(register("c1", 4242, "abc123"), start, AT);
        chat.poll("c1", "poll-1", start, AT).unwrap();
        assert!(
            chat.tick(start + Duration::from_secs(24), AT)
                .answers
                .is_empty()
        );
        let effects = chat.tick(start + Duration::from_secs(25), AT);
        assert_eq!(effects.answers, [("poll-1", Inbox::default())]);
        assert!(effects.events.is_empty());

        // The channel was killed: no new poll comes.
        let effects = chat.tick(start + Duration::from_secs(35), AT);
        assert_eq!(effects.events, [unlinked("abc123")]);
        assert_eq!(
            chat.reply("c1", "hi", start, AT).unwrap_err(),
            ChatError::UnknownChannel
        );
        assert!(chat.poll("c1", "poll-2", start, AT).is_err());
    }

    #[test]
    fn a_second_poll_releases_the_first_and_unregister_releases_the_last() {
        let now = Instant::now();
        let mut chat = Test::new(limits());
        chat.register(register("c1", 4242, "abc123"), now, AT);
        chat.poll("c1", "poll-1", now, AT).unwrap();
        let effects = chat.poll("c1", "poll-2", now, AT).unwrap();
        assert_eq!(effects.answers, [("poll-1", Inbox::default())]);
        let effects = chat.unregister("c1", AT).unwrap();
        assert_eq!(effects.answers, [("poll-2", Inbox::default())]);
        assert_eq!(effects.events, [unlinked("abc123")]);
        assert_eq!(
            chat.unregister("c1", AT).unwrap_err(),
            ChatError::UnknownChannel
        );
    }

    #[test]
    fn the_newest_channel_of_a_process_serves_its_session() {
        let now = Instant::now();
        let mut chat = Test::new(limits());
        chat.observe(Some(4242), Hook::SessionStart, "abc123", AT);
        chat.register(register("old", 4242, "abc123"), now, AT);
        // Claude Code restarted the server (/mcp reconnect) before the old one unregistered.
        assert!(
            chat.register(register("new", 4242, "abc123"), now, AT)
                .events
                .is_empty()
        );
        chat.poll("old", "poll-old", now, AT).unwrap();
        chat.poll("new", "poll-new", now, AT).unwrap();
        let effects = chat.send("abc123", message("m1", "hi"), AT).unwrap();
        assert_eq!(effects.answers[0].0, "poll-new");
        assert!(chat.unregister("new", AT).unwrap().events.is_empty());
        assert_eq!(
            chat.linked_sessions(),
            ["abc123"],
            "the older one takes over"
        );
    }

    #[test]
    fn replies_need_a_session_and_text() {
        let now = Instant::now();
        let mut chat = Test::new(limits());
        chat.observe(Some(4242), Hook::SessionStart, "abc123", AT);
        chat.register(register("c1", 4242, "abc123"), now, AT);
        assert!(matches!(
            chat.reply("c1", "  ", now, AT),
            Err(ChatError::Invalid(_))
        ));
        assert!(matches!(
            chat.send("abc123", message("", "hi"), AT),
            Err(ChatError::Invalid(_))
        ));
        chat.observe(Some(4242), Hook::SessionEnd, "abc123", AT);
        assert_eq!(
            chat.reply("c1", "done", now, AT).unwrap_err(),
            ChatError::NoSession
        );
    }

    #[test]
    fn processes_without_channels_are_not_kept_forever() {
        let mut chat = Test::new(limits());
        for pid in 1..=(PROCESSES_KEPT as u32 + 50) {
            chat.observe(Some(pid), Hook::SessionStart, &format!("s{pid}"), AT);
        }
        assert_eq!(chat.processes.len(), PROCESSES_KEPT);
        assert!(
            !chat.processes.contains_key(&1),
            "the least recently heard goes first"
        );
        assert!(chat.processes.contains_key(&300));
        chat.observe(Some(300), Hook::SessionEnd, "s300", AT);
        assert!(
            !chat.processes.contains_key(&300),
            "ended, and no channel needs it"
        );
    }
}
