//! kiko-channel: Kiko's Claude Code channel (https://code.claude.com/docs/en/channels-reference).
//!
//! Claude Code starts it as a stdio MCP server, one per Claude Code process, but only in the sessions
//! launched with Kiko's mcp.json (see `protocol::launch_args`); other sessions never start it. It
//! declares the `claude/channel` capability, connects to Kiko's local server (127.0.0.1, same port and
//! token as the hooks), pushes the messages the user types in Kiko into the session, and gives Claude a
//! `reply` tool to answer. It only ever receives messages from Kiko's server: it opens no port of its
//! own. Kiko links it to a session through its parent process, the same Claude Code process whose
//! hooks report the session it runs now, so the chat follows `/clear` and `/resume`.
//!
//! The `protocol` module is also used by Kiko's server, so both ends share one contract.

pub mod bridge;
pub mod kiko;
pub mod mcp;
pub mod platform;
pub mod protocol;
