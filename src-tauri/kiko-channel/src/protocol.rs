//! The contract between Kiko's local server and kiko-channel. Both sides build their requests and
//! answers from these types, so they can't drift apart.
//!
//! Every request is a POST to `http://127.0.0.1:<port>/kiko/channel/<endpoint>` with a JSON body and
//! the `X-Kiko-Token` header, the same token as the hooks (from Kiko's config.json).

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The MCP server name Kiko registers with `claude mcp add`; Claude Code tags the channel's events
/// with it (`<channel source="kiko">`).
pub const SERVER_NAME: &str = "kiko";
/// The `--dangerously-load-development-channels` entry that turns this MCP server into a channel.
pub const CHANNEL_ENTRY: &str = "server:kiko";
/// The research-preview flag that loads a channel which is not on Anthropic's allowlist.
pub const DEVELOPMENT_CHANNELS_FLAG: &str = "--dangerously-load-development-channels";

pub const TOKEN_HEADER: &str = "X-Kiko-Token";
pub const CHANNEL_PATH_PREFIX: &str = "/kiko/channel/";

/// Kiko answers a poll with no message after this long; the channel polls again at once.
pub const POLL_TIMEOUT: Duration = Duration::from_secs(25);

/// The channel endpoints, `/kiko/channel/<path>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endpoint {
    /// A channel started in a session: [`Register`].
    Register,
    /// Waits for the user's messages to that session: [`ChannelRef`] → [`Inbox`].
    Poll,
    /// Claude's answer, from the `reply` tool: [`Reply`].
    Reply,
    /// The session closed the channel: [`ChannelRef`].
    Unregister,
}

impl Endpoint {
    pub const ALL: [Endpoint; 4] = [
        Endpoint::Register,
        Endpoint::Poll,
        Endpoint::Reply,
        Endpoint::Unregister,
    ];

    pub fn path(self) -> &'static str {
        match self {
            Endpoint::Register => "register",
            Endpoint::Poll => "poll",
            Endpoint::Reply => "reply",
            Endpoint::Unregister => "unregister",
        }
    }

    pub fn from_path(path: &str) -> Option<Endpoint> {
        Endpoint::ALL.into_iter().find(|e| e.path() == path)
    }

    /// `/kiko/channel/<path>`
    pub fn url_path(self) -> String {
        format!("{CHANNEL_PATH_PREFIX}{}", self.path())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Register {
    /// Random id of this channel process, used in every later request.
    pub channel: String,
    /// `CLAUDE_CODE_SESSION_ID` as Claude Code set it when it started the channel.
    pub session_id: String,
    pub cwd: String,
    /// The channel's process id, to find the Claude Code process that started it.
    pub pid: u32,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRef {
    pub channel: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub channel: String,
    pub text: String,
}

/// A message the user typed in Kiko for the session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub text: String,
    /// Epoch ms.
    pub at: u64,
}

/// Answer to a poll: oldest first, empty when the poll timed out.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inbox {
    pub messages: Vec<ChatMessage>,
}

/// What the channel needs from Kiko's config.json; the other fields are Kiko's own.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub port: u16,
    #[serde(default)]
    pub token: String,
}
