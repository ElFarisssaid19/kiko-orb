//! Claude Code hook payloads → Kiko events, and Kiko decisions → hook responses.
//! Shapes follow the hooks reference: https://code.claude.com/docs/en/hooks

use serde_json::{Map, Value, json};

use super::events::{
    PermissionRequest, Question, QuestionOption, QuestionRequest, SessionRef, Step, StepStatus,
    SubjectKind,
};

/// The hook events Kiko listens to, one HTTP endpoint each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hook {
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    PermissionRequest,
    Notification,
    Stop,
    StopFailure,
}

impl Hook {
    pub const ALL: [Hook; 10] = [
        Hook::SessionStart,
        Hook::SessionEnd,
        Hook::UserPromptSubmit,
        Hook::PreToolUse,
        Hook::PostToolUse,
        Hook::PostToolUseFailure,
        Hook::PermissionRequest,
        Hook::Notification,
        Hook::Stop,
        Hook::StopFailure,
    ];

    /// Event name in settings.json and in `hook_event_name`.
    pub fn event_name(self) -> &'static str {
        match self {
            Hook::SessionStart => "SessionStart",
            Hook::SessionEnd => "SessionEnd",
            Hook::UserPromptSubmit => "UserPromptSubmit",
            Hook::PreToolUse => "PreToolUse",
            Hook::PostToolUse => "PostToolUse",
            Hook::PostToolUseFailure => "PostToolUseFailure",
            Hook::PermissionRequest => "PermissionRequest",
            Hook::Notification => "Notification",
            Hook::Stop => "Stop",
            Hook::StopFailure => "StopFailure",
        }
    }

    /// Last segment of the endpoint, `/kiko/hooks/<path>`.
    pub fn path(self) -> &'static str {
        match self {
            Hook::SessionStart => "session-start",
            Hook::SessionEnd => "session-end",
            Hook::UserPromptSubmit => "user-prompt-submit",
            Hook::PreToolUse => "pre-tool-use",
            Hook::PostToolUse => "post-tool-use",
            Hook::PostToolUseFailure => "post-tool-use-failure",
            Hook::PermissionRequest => "permission-request",
            Hook::Notification => "notification",
            Hook::Stop => "stop",
            Hook::StopFailure => "stop-failure",
        }
    }

    pub fn from_path(path: &str) -> Option<Hook> {
        Hook::ALL.into_iter().find(|hook| hook.path() == path)
    }
}

/// The tool whose PreToolUse Kiko answers (with the user's choices) instead of just observing it.
pub const ASK_USER_QUESTION: &str = "AskUserQuestion";

/// Longest command, path or summary sent to the island.
const MAX_TEXT: usize = 200;

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut short: String = text.chars().take(max - 1).collect();
    short.push('…');
    short
}

pub fn session_ref(input: &Value) -> SessionRef {
    let cwd = text(input, "cwd").to_string();
    let project = cwd
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("unknown")
        .to_string();
    SessionRef {
        id: text(input, "session_id").to_string(),
        project,
        cwd,
    }
}

pub fn tool_name(input: &Value) -> &str {
    text(input, "tool_name")
}

pub fn tool_input(input: &Value) -> &Value {
    input.get("tool_input").unwrap_or(&Value::Null)
}

/// What a tool call acts on, for the island's step list.
pub fn tool_target(tool: &str, tool_input: &Value) -> String {
    let target = match tool {
        "Bash" | "PowerShell" => text(tool_input, "command")
            .lines()
            .next()
            .unwrap_or("")
            .trim(),
        "Edit" | "Write" | "Read" | "MultiEdit" => text(tool_input, "file_path"),
        "NotebookEdit" => text(tool_input, "notebook_path"),
        "Grep" | "Glob" => text(tool_input, "pattern"),
        "WebFetch" => text(tool_input, "url"),
        "WebSearch" => text(tool_input, "query"),
        "Agent" | "Task" => text(tool_input, "description"),
        ASK_USER_QUESTION => tool_input
            .get("questions")
            .and_then(|q| q.get(0))
            .map(|q| text(q, "question"))
            .unwrap_or(""),
        _ => "",
    };
    truncate(target, MAX_TEXT)
}

pub fn step(input: &Value, status: StepStatus) -> Step {
    let tool = tool_name(input);
    Step {
        id: text(input, "tool_use_id").to_string(),
        tool: tool.to_string(),
        target: tool_target(tool, tool_input(input)),
        status,
    }
}

fn count_lines(text: &str) -> u32 {
    text.lines().count() as u32
}

/// Lines added and removed when `old` becomes `new`: lines shared at the start and the end are not counted.
fn diff_lines(old: &str, new: &str) -> (u32, u32) {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    (
        (new.len() - prefix - suffix) as u32,
        (old.len() - prefix - suffix) as u32,
    )
}

/// Lines added and removed by a successful file tool, computed from its input.
/// `Write` reports its whole content as added: the previous file is not part of the input.
pub fn line_changes(tool: &str, tool_input: &Value) -> (u32, u32) {
    match tool {
        "Edit" => diff_lines(
            text(tool_input, "old_string"),
            text(tool_input, "new_string"),
        ),
        // Not in the current tools reference; kept for older Claude Code versions that still send it.
        "MultiEdit" => tool_input
            .get("edits")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|edit| diff_lines(text(edit, "old_string"), text(edit, "new_string")))
            .fold((0, 0), |(a, r), (da, dr)| (a + da, r + dr)),
        "Write" => (count_lines(text(tool_input, "content")), 0),
        _ => (0, 0),
    }
}

/// The permission updates "Always" sends back: Claude Code's own suggestions that allow a rule
/// or accept edits. The docs allow echoing a suggestion as `updatedPermissions`.
pub fn always_allow_updates(input: &Value) -> Vec<Value> {
    input
        .get("permission_suggestions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|s| {
            let kind = text(s, "type");
            (kind == "addRules" && text(s, "behavior") == "allow")
                || (kind == "setMode" && text(s, "mode") == "acceptEdits")
        })
        .cloned()
        .collect()
}

pub fn permission_request(id: &str, input: &Value) -> PermissionRequest {
    let tool = tool_name(input);
    let tool_input = tool_input(input);
    let (kind, subject) = match tool {
        "Bash" | "PowerShell" => (
            SubjectKind::Command,
            text(tool_input, "command").to_string(),
        ),
        "Edit" | "Write" | "MultiEdit" => {
            (SubjectKind::File, text(tool_input, "file_path").to_string())
        }
        "NotebookEdit" => (
            SubjectKind::File,
            text(tool_input, "notebook_path").to_string(),
        ),
        _ => {
            let target = tool_target(tool, tool_input);
            let summary = if target.is_empty() {
                tool_input.to_string()
            } else {
                target
            };
            (SubjectKind::Other, truncate(&summary, MAX_TEXT))
        }
    };
    PermissionRequest {
        id: id.to_string(),
        session_id: text(input, "session_id").to_string(),
        tool: tool.to_string(),
        kind,
        subject,
        can_always_allow: !always_allow_updates(input).is_empty(),
    }
}

pub fn question_request(id: &str, input: &Value) -> QuestionRequest {
    let questions = tool_input(input)
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|q| Question {
            question: text(q, "question").to_string(),
            header: text(q, "header").to_string(),
            options: q
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|o| QuestionOption {
                    label: text(o, "label").to_string(),
                    description: text(o, "description").to_string(),
                })
                .collect(),
            multi_select: q
                .get("multiSelect")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .collect();
    QuestionRequest {
        id: id.to_string(),
        session_id: text(input, "session_id").to_string(),
        questions,
    }
}

/// The user's choice on a permission card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Allow,
    Always,
    Deny,
}

/// PermissionRequest response body. "Always" is an allow that echoes Claude Code's suggested rule.
pub fn permission_response(decision: PermissionDecision, always_updates: &[Value]) -> Value {
    let decision = match decision {
        PermissionDecision::Allow => json!({ "behavior": "allow" }),
        PermissionDecision::Always if !always_updates.is_empty() => {
            json!({ "behavior": "allow", "updatedPermissions": always_updates })
        }
        PermissionDecision::Always => json!({ "behavior": "allow" }),
        PermissionDecision::Deny => {
            json!({ "behavior": "deny", "message": "The user denied this in Kiko." })
        }
    };
    json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } })
}

/// PreToolUse response that answers an AskUserQuestion call: allow, with the original input plus `answers`.
pub fn question_response(original_input: &Value, answers: &Map<String, Value>) -> Value {
    let mut updated = original_input.as_object().cloned().unwrap_or_default();
    updated.insert("answers".into(), Value::Object(answers.clone()));
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": updated
        }
    })
}

/// One line for the debug log.
pub fn summary(hook: Hook, input: &Value) -> String {
    let tool = tool_name(input);
    let target = tool_target(tool, tool_input(input));
    let with_target = |prefix: &str| {
        if target.is_empty() {
            prefix.to_string()
        } else {
            format!("{prefix}: {target}")
        }
    };
    let line = match hook {
        Hook::SessionStart => format!("source {}", text(input, "source")),
        Hook::SessionEnd => format!("reason {}", text(input, "reason")),
        Hook::UserPromptSubmit => text(input, "prompt").to_string(),
        Hook::PreToolUse | Hook::PermissionRequest => with_target(tool),
        Hook::PostToolUse => {
            let (added, removed) = line_changes(tool, tool_input(input));
            if added + removed > 0 {
                format!("{} (+{added} −{removed})", with_target(tool))
            } else {
                with_target(tool)
            }
        }
        Hook::PostToolUseFailure => format!("{} failed", with_target(tool)),
        Hook::Notification => format!(
            "{}: {}",
            text(input, "notification_type"),
            text(input, "message")
        ),
        Hook::Stop => "turn ended".to_string(),
        Hook::StopFailure => format!("API error: {}", text(input, "error")),
    };
    truncate(line.lines().next().unwrap_or(""), 120)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Common input example from the hooks reference (PreToolUse on Bash).
    fn pre_tool_use_bash() -> Value {
        json!({
          "session_id": "abc123",
          "prompt_id": "550e8400-e29b-41d4-a716-446655440000",
          "transcript_path": "/home/user/.claude/projects/.../transcript.jsonl",
          "cwd": "/home/user/my-project",
          "scratchpad_dir": "/tmp/claude-1000/-home-user-my-project/abc123/scratchpad",
          "permission_mode": "default",
          "hook_event_name": "PreToolUse",
          "tool_name": "Bash",
          "tool_input": {
            "command": "npm test",
            "description": "Run test suite",
            "timeout": 120000,
            "run_in_background": false
          },
          "tool_use_id": "toolu_01ABC123..."
        })
    }

    /// PermissionRequest input example from the hooks reference.
    fn permission_request_bash() -> Value {
        json!({
          "session_id": "abc123",
          "transcript_path": "/Users/.../.claude/projects/.../00893aaf-19fa-41d2-8238-13269b9b3ca0.jsonl",
          "cwd": "/Users/...",
          "permission_mode": "default",
          "hook_event_name": "PermissionRequest",
          "tool_name": "Bash",
          "tool_input": {
            "command": "rm -rf node_modules",
            "description": "Remove node_modules directory"
          },
          "permission_suggestions": [
            {
              "type": "addRules",
              "rules": [{ "toolName": "Bash", "ruleContent": "rm -rf node_modules" }],
              "behavior": "allow",
              "destination": "localSettings"
            }
          ]
        })
    }

    #[test]
    fn session_and_step_from_the_docs_example() {
        let input = pre_tool_use_bash();
        let session = session_ref(&input);
        assert_eq!(session.id, "abc123");
        assert_eq!(session.project, "my-project");
        let step = step(&input, StepStatus::Running);
        assert_eq!(step.id, "toolu_01ABC123...");
        assert_eq!(step.tool, "Bash");
        assert_eq!(step.target, "npm test");
    }

    #[test]
    fn line_changes_for_file_tools() {
        // PostToolUse Write example from the docs: one line of content.
        let write = json!({ "file_path": "/path/to/file.txt", "content": "file content" });
        assert_eq!(line_changes("Write", &write), (1, 0));
        // Edit fields from the docs table: only the differing lines count.
        let edit = json!({
            "file_path": "/path/to/file.txt",
            "old_string": "a\nb\nc",
            "new_string": "a\nB\nB2\nc",
            "replace_all": false
        });
        assert_eq!(line_changes("Edit", &edit), (2, 1));
        assert_eq!(line_changes("Read", &json!({ "file_path": "/x" })), (0, 0));
    }

    #[test]
    fn permission_request_and_always_from_the_docs_example() {
        let input = permission_request_bash();
        let request = permission_request("r1", &input);
        assert_eq!(request.kind, SubjectKind::Command);
        assert_eq!(request.subject, "rm -rf node_modules");
        assert!(request.can_always_allow);

        let updates = always_allow_updates(&input);
        assert_eq!(
            permission_response(PermissionDecision::Always, &updates),
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
        assert_eq!(
            permission_response(PermissionDecision::Allow, &updates)["hookSpecificOutput"]["decision"],
            json!({ "behavior": "allow" })
        );
        assert_eq!(
            permission_response(PermissionDecision::Deny, &updates)["hookSpecificOutput"]["decision"]
                ["behavior"],
            "deny"
        );
    }

    #[test]
    fn ask_user_question_answer_matches_the_docs() {
        // AskUserQuestion tool_input from the docs table.
        let input = json!({
            "session_id": "abc123",
            "cwd": "/home/user/my-project",
            "hook_event_name": "PreToolUse",
            "tool_name": "AskUserQuestion",
            "tool_input": {
                "questions": [{
                    "question": "Which framework?",
                    "header": "Framework",
                    "options": [
                        { "label": "React", "description": "Component library" },
                        { "label": "Vue", "description": "Progressive framework" }
                    ],
                    "multiSelect": false
                }]
            },
            "tool_use_id": "toolu_02"
        });
        let request = question_request("q1", &input);
        assert_eq!(request.questions.len(), 1);
        assert_eq!(request.questions[0].options[1].label, "Vue");

        let mut answers = Map::new();
        answers.insert("Which framework?".into(), json!("React"));
        // The docs' example response, field for field.
        assert_eq!(
            question_response(tool_input(&input), &answers),
            json!({
              "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "allow",
                "updatedInput": {
                  "questions": [{
                    "question": "Which framework?",
                    "header": "Framework",
                    "options": [{"label": "React", "description": "Component library"}, {"label": "Vue", "description": "Progressive framework"}],
                    "multiSelect": false
                  }],
                  "answers": {"Which framework?": "React"}
                }
              }
            })
        );
    }

    #[test]
    fn endpoints_round_trip() {
        for hook in Hook::ALL {
            assert_eq!(Hook::from_path(hook.path()), Some(hook));
        }
        assert_eq!(Hook::from_path("nope"), None);
    }
}
