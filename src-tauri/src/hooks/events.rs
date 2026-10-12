//! Events sent to the frontend. Mirrors `AgentEvent` in `src/types/events.ts` (camelCase JSON).

use serde::Serialize;

/// The session an event belongs to. Every session-scoped event carries it, so the frontend can
/// pick up sessions that started before Kiko did.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    pub id: String,
    /// Basename of `cwd`.
    pub project: String,
    pub cwd: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Running,
    Done,
    Error,
}

/// One tool call, e.g. `Edit` on `src/App.tsx`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// `tool_use_id`, shared by the PreToolUse and PostToolUse of the same call.
    pub id: String,
    pub tool: String,
    /// What the tool acts on: a command, a file path, a pattern… Empty when there is nothing to show.
    pub target: String,
    pub status: StepStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SubjectKind {
    Command,
    File,
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: String,
    pub session_id: String,
    pub tool: String,
    pub kind: SubjectKind,
    /// The command for Bash, the file for edits, otherwise a short summary of the tool input.
    pub subject: String,
    /// Claude Code suggested a rule to remember this choice, so "Always" is offered.
    pub can_always_allow: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

/// An `AskUserQuestion` call: one to four questions.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRequest {
    pub id: String,
    pub session_id: String,
    pub questions: Vec<Question>,
}

/// Why a held request was let go.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolveReason {
    /// The user decided in Kiko.
    Answered,
    /// "Reply in terminal": Claude Code shows its own prompt.
    Released,
    Timeout,
    /// The session's turn ended (Stop, StopFailure, a new prompt) while the request waited.
    TurnEnded,
    SessionEnded,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum AgentEvent {
    SessionStarted {
        at: u64,
        session: SessionRef,
        source: String,
    },
    SessionEnded {
        at: u64,
        session: SessionRef,
        reason: String,
    },
    PromptSubmitted {
        at: u64,
        session: SessionRef,
    },
    ToolStarted {
        at: u64,
        session: SessionRef,
        step: Step,
    },
    ToolFinished {
        at: u64,
        session: SessionRef,
        step: Step,
        lines_added: u32,
        lines_removed: u32,
    },
    PermissionRequested {
        at: u64,
        session: SessionRef,
        request: PermissionRequest,
    },
    QuestionAsked {
        at: u64,
        session: SessionRef,
        request: QuestionRequest,
    },
    RequestResolved {
        at: u64,
        request_id: String,
        reason: ResolveReason,
    },
    Notification {
        at: u64,
        session: SessionRef,
        notification_type: String,
        message: String,
    },
    TurnStopped {
        at: u64,
        session: SessionRef,
    },
    TurnFailed {
        at: u64,
        session: SessionRef,
        error: String,
    },
}

/// One line of the debug log shown in the playground.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookLogEntry {
    pub at: u64,
    /// Hook event name as Claude Code sends it, e.g. `PreToolUse`, or `Rejected`.
    pub event: String,
    pub session_id: String,
    pub project: String,
    pub summary: String,
}
