//! Chat with a running Claude Code session through a channel (kiko-channel, see its crate): which
//! channel serves which session, and the installer that sets chat up for the sessions the user starts
//! with Kiko's launch command. The local server (`crate::hooks`) serves the channels' endpoints.

mod install;
mod registry;

pub use install::{
    ChatInstaller, ChatPreview, ChatState, ClaudeCli, MIN_CLAUDE_VERSION, claude_version,
    find_claude,
};
pub use registry::{ChatError, Effects, Limits, Registry};
