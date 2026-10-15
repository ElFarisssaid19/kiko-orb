// Events from Claude Code, as Kiko's hook server (src-tauri/src/hooks/) sends them.
// Mirrors src-tauri/src/hooks/events.rs.

export type AgentKind = "claude-code";

/** The session an event belongs to. `project` is the basename of `cwd`. */
export interface SessionRef {
  id: string;
  project: string;
  cwd: string;
}

export type StepStatus = "running" | "done" | "error";

/** A tool call, e.g. { tool: "Edit", target: "src/App.tsx" }. `id` is Claude Code's tool_use_id. */
export interface Step {
  id: string;
  tool: string;
  /** What the tool acts on: a command, a file path, a pattern… Empty when there is nothing to show. */
  target: string;
  status: StepStatus;
}

/** The agent waits for approval to run a command, edit a file or use another tool. */
export interface PermissionRequest {
  id: string;
  sessionId: string;
  tool: string;
  kind: "command" | "file" | "other";
  /** The command for Bash, the file for edits, otherwise a short summary of the tool input. */
  subject: string;
  /** Claude Code suggested a rule to remember the choice, so "Always" is offered. */
  canAlwaysAllow: boolean;
}

export type PermissionDecision = "allow" | "always" | "deny";

export interface QuestionOption {
  label: string;
  description: string;
}

export interface Question {
  question: string;
  /** Short label, e.g. "Framework". */
  header: string;
  options: QuestionOption[];
  multiSelect: boolean;
}

/** An AskUserQuestion call: one to four questions. */
export interface QuestionRequest {
  id: string;
  sessionId: string;
  questions: Question[];
}

/** The user's answer to a held request. */
export type RequestResponse =
  | { kind: "permission"; decision: PermissionDecision }
  /** Question text → chosen label, several labels joined with ", ". */
  | { kind: "question"; answers: Record<string, string> }
  /** Reply in the terminal: Claude Code shows its own prompt. */
  | { kind: "release" };

/** A chat message: one the user wrote in Kiko, or Claude's reply. Mirrors `ChatMessage` in kiko-channel's protocol.rs. */
export interface ChatMessage {
  id: string;
  text: string;
  /** Epoch ms. */
  at: number;
}

/** Why a held request went away. */
export type ResolveReason = "answered" | "released" | "timeout" | "turn-ended" | "session-ended";

export type AgentEvent =
  | { type: "session-started"; at: number; session: SessionRef; source: string }
  | { type: "session-ended"; at: number; session: SessionRef; reason: string }
  | { type: "prompt-submitted"; at: number; session: SessionRef }
  | { type: "tool-started"; at: number; session: SessionRef; step: Step }
  | {
      type: "tool-finished";
      at: number;
      session: SessionRef;
      step: Step;
      linesAdded: number;
      linesRemoved: number;
    }
  | { type: "permission-requested"; at: number; session: SessionRef; request: PermissionRequest }
  | { type: "question-asked"; at: number; session: SessionRef; request: QuestionRequest }
  | { type: "request-resolved"; at: number; requestId: string; reason: ResolveReason }
  | { type: "notification"; at: number; session: SessionRef; notificationType: string; message: string }
  | { type: "turn-stopped"; at: number; session: SessionRef }
  | { type: "turn-failed"; at: number; session: SessionRef; error: string }
  /** A chat channel now serves this session: the user can write to it from Kiko. */
  | { type: "chat-linked"; at: number; sessionId: string }
  /** The session has no channel any more (it ended, was cleared, or its Claude Code exited). */
  | { type: "chat-unlinked"; at: number; sessionId: string }
  /** The channel picked up these messages and handed them to Claude Code. */
  | { type: "chat-delivered"; at: number; sessionId: string; messageIds: string[] }
  /** Claude answered with the channel's reply tool. */
  | { type: "chat-reply"; at: number; sessionId: string; message: ChatMessage };

/** One line of the hook server's debug log. */
export interface HookLogEntry {
  at: number;
  /** Hook event name, e.g. "PreToolUse"; "Kiko" for Kiko's own answers; "Rejected" for refused requests. */
  event: string;
  sessionId: string;
  project: string;
  summary: string;
}
