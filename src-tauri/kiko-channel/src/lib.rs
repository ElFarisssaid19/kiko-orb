//! kiko-channel: Kiko's Claude Code channel (https://code.claude.com/docs/en/channels-reference).
//!
//! Claude Code starts it as a stdio MCP server, one per session. It declares the `claude/channel`
//! capability, connects to Kiko's local server (127.0.0.1, same port and token as the hooks), pushes
//! the messages the user types in Kiko into the session, and gives Claude a `reply` tool to answer.
//! It only ever receives messages from Kiko's server: it opens no port of its own.
//!
//! The `protocol` module is also used by Kiko's server, so both ends share one contract.

pub mod bridge;
pub mod kiko;
pub mod mcp;
pub mod protocol;
