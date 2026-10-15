//! Claude Code integration: the local server (hooks, and the chat channels of `crate::chat`) and the
//! installer for Kiko's hooks.

mod config;
#[cfg(test)]
mod curl_tests;
mod events;
mod install;
mod parse;
mod server;

pub use config::{KikoConfig, auth_header_path};
pub use events::{AgentEvent, HookLogEntry};
pub use install::{Action, Applied, Installer, Preview, claude_settings_path, shell_path};
pub use parse::Hook;
pub use server::{Emitted, HookServer, UserResponse};
