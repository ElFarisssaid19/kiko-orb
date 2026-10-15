//! The MCP side of kiko-channel: JSON-RPC 2.0 over stdio, one JSON message per line.
//! Shapes follow the channels reference: https://code.claude.com/docs/en/channels-reference

use serde_json::{Value, json};

use crate::protocol::{ChatMessage, SERVER_NAME};

/// Revisions answered with the `initialize` handshake, newest first. Revision 2026-07-28 is left out
/// on purpose: Claude Code doesn't register a channel server that negotiates it (see "Push messages
/// with channels" in https://code.claude.com/docs/en/mcp), so its `server/discover` probe gets
/// "method not found" and Claude Code falls back to `initialize`.
pub const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

pub const REPLY_TOOL: &str = "reply";

/// Given to Claude as context when the server connects.
pub const INSTRUCTIONS: &str = "Kiko is a desktop companion app on this computer that shows this \
Claude Code session. The user who runs this session can also write to you from Kiko: those messages \
arrive as <channel source=\"kiko\" message_id=\"...\">message</channel>. Treat each one as a message \
from the user, like a prompt typed in the terminal. The user reads your answers in Kiko, not in the \
terminal: once you have handled a Kiko message, send your answer with the reply tool, as one complete \
message (Markdown is fine; put code in fenced code blocks). Only use the reply tool to answer Kiko \
messages.";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Answers Claude Code's requests. `send_reply` delivers the `reply` tool's text to Kiko.
pub struct McpServer<F> {
    send_reply: F,
    initialized: bool,
}

impl<F: FnMut(&str) -> Result<(), String>> McpServer<F> {
    pub fn new(send_reply: F) -> Self {
        Self {
            send_reply,
            initialized: false,
        }
    }

    /// Claude Code finished the handshake (`notifications/initialized`): channel events may be sent.
    pub fn initialized(&self) -> bool {
        self.initialized
    }

    /// Handles one line from stdin. Returns the message to write back, if any (requests get one,
    /// notifications don't).
    pub fn handle_line(&mut self, line: &str) -> Option<Value> {
        match serde_json::from_str::<Value>(line) {
            Ok(message) => self.handle(message),
            Err(error) => Some(error_response(
                Value::Null,
                PARSE_ERROR,
                &format!("Parse error: {error}"),
            )),
        }
    }

    fn handle(&mut self, message: Value) -> Option<Value> {
        let Some(object) = message.as_object() else {
            return Some(error_response(
                Value::Null,
                INVALID_REQUEST,
                "Expected a JSON-RPC message object",
            ));
        };
        let id = object.get("id").cloned().filter(|id| !id.is_null());
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            // A response to a request of ours (there are none), or garbage: nothing to answer.
            return id
                .filter(|_| !object.contains_key("result") && !object.contains_key("error"))
                .map(|id| error_response(id, INVALID_REQUEST, "Missing method"));
        };
        let params = object.get("params").cloned().unwrap_or(Value::Null);

        let Some(id) = id else {
            if method == "notifications/initialized" {
                self.initialized = true;
            }
            return None;
        };
        Some(match method {
            "initialize" => result(id, initialize_result(&params)),
            "ping" => result(id, json!({})),
            "tools/list" => result(id, json!({ "tools": [reply_tool()] })),
            "tools/call" => self.call_tool(id, &params),
            // Includes `server/discover`, the 2026-07-28 probe.
            _ => error_response(id, METHOD_NOT_FOUND, &format!("Method not found: {method}")),
        })
    }

    fn call_tool(&mut self, id: Value, params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        if name != REPLY_TOOL {
            return error_response(id, INVALID_PARAMS, &format!("Unknown tool: {name}"));
        }
        let Some(text) = params
            .pointer("/arguments/text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
        else {
            return result(id, tool_text("The reply needs a non-empty `text`.", true));
        };
        match (self.send_reply)(text) {
            Ok(()) => result(id, tool_text("Sent to Kiko.", false)),
            Err(why) => result(
                id,
                tool_text(
                    &format!(
                        "Kiko did not receive the reply: {why}. Give the user your answer in the terminal instead."
                    ),
                    true,
                ),
            ),
        }
    }
}

fn initialize_result(params: &Value) -> Value {
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or("");
    // The client's version when we speak it, else our newest: the client decides whether to go on.
    let version = PROTOCOL_VERSIONS
        .into_iter()
        .find(|v| *v == requested)
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": {
            // Makes this server a channel. No `claude/channel/permission`: Kiko answers permissions
            // through its hooks.
            "experimental": { "claude/channel": {} },
            "tools": {}
        },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS
    })
}

fn reply_tool() -> Value {
    json!({
        "name": REPLY_TOOL,
        "title": "Reply in Kiko",
        "description": "Send your answer to the user in Kiko, the desktop app their <channel source=\"kiko\"> messages come from. Call it once you have handled such a message, with one complete answer. Markdown, with fenced code blocks for code.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Your answer to the user, in Markdown." }
            },
            "required": ["text"],
            "additionalProperties": false
        }
    })
}

/// `notifications/claude/channel` for a message the user typed in Kiko.
pub fn channel_notification(message: &ChatMessage) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/claude/channel",
        "params": {
            "content": message.text,
            // Becomes <channel source="kiko" message_id="…">. Keys must be identifiers.
            "meta": { "message_id": message.id }
        }
    })
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn tool_text(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> McpServer<impl FnMut(&str) -> Result<(), String>> {
        McpServer::new(|_: &str| Ok(()))
    }

    #[test]
    fn the_2026_probe_falls_back_to_initialize() {
        let mut server = server();
        let probe = server
            .handle_line(r#"{"jsonrpc":"2.0","id":0,"method":"server/discover","params":{}}"#)
            .unwrap();
        assert_eq!(probe["error"]["code"], METHOD_NOT_FOUND);

        let init = server.handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"claude-code","version":"2.1.296"}}}"#,
        ).unwrap();
        assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    }

    #[test]
    fn a_known_version_is_echoed() {
        let init = server()
            .handle_line(r#"{"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#)
            .unwrap();
        assert_eq!(init["id"], "a");
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(init["result"]["capabilities"]["experimental"]["claude/channel"], json!({}));
        assert!(init["result"]["capabilities"]["experimental"].get("claude/channel/permission").is_none());
    }

    #[test]
    fn notifications_get_no_answer() {
        let mut server = server();
        assert!(!server.initialized());
        assert_eq!(
            server.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#),
            None
        );
        assert!(server.initialized());
        assert_eq!(
            server.handle_line(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":3}}"#),
            None
        );
    }

    #[test]
    fn garbage_gets_json_rpc_errors() {
        let mut server = server();
        assert_eq!(server.handle_line("{nope").unwrap()["error"]["code"], PARSE_ERROR);
        assert_eq!(server.handle_line("[1,2]").unwrap()["error"]["code"], INVALID_REQUEST);
        assert_eq!(
            server.handle_line(r#"{"jsonrpc":"2.0","id":4,"method":"resources/list"}"#).unwrap()["error"]["code"],
            METHOD_NOT_FOUND
        );
    }

    #[test]
    fn the_reply_tool_reports_failures_to_claude() {
        let mut sent = Vec::new();
        let mut ok = McpServer::new(|text: &str| {
            sent.push(text.to_string());
            Ok(())
        });
        let call = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"reply","arguments":{"text":"All tests pass."}}}"#;
        assert_eq!(ok.handle_line(call).unwrap()["result"]["isError"], false);
        // `ok` isn't used again, so its borrow of `sent` ends here.
        assert_eq!(sent, ["All tests pass."]);

        let mut down = McpServer::new(|_: &str| Err("Kiko is not running".to_string()));
        let answer = down.handle_line(call).unwrap();
        assert_eq!(answer["result"]["isError"], true);
        assert!(answer["result"]["content"][0]["text"].as_str().unwrap().contains("Kiko is not running"));

        let empty = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"reply","arguments":{"text":"  "}}}"#;
        assert_eq!(down.handle_line(empty).unwrap()["result"]["isError"], true);
    }

    #[test]
    fn channel_events_carry_the_message_id() {
        let event = channel_notification(&ChatMessage {
            id: "m1".into(),
            text: "line one\nline two".into(),
            at: 0,
        });
        assert_eq!(event["method"], "notifications/claude/channel");
        assert_eq!(event["params"]["content"], "line one\nline two");
        assert_eq!(event["params"]["meta"]["message_id"], "m1");
        // One message per line on stdio.
        assert!(!event.to_string().contains('\n'));
    }
}
