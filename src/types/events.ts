// Event model shared by the EventSource implementations and the store.
// Shapes follow what Claude Code hooks can report; the real source arrives in a later step.

export type AgentKind = "claude-code";

/** One running agent session. */
export interface Session {
  id: string;
  project: string;
  agent: AgentKind;
  /** Human-readable current activity, e.g. "Editing app.tsx". */
  currentStep: string;
  /** Epoch ms. */
  startedAt: number;
  linesAdded: number;
  linesRemoved: number;
}

export type StepStatus = "running" | "done" | "error";

/** A tool call made by an agent, e.g. { tool: "Edit", target: "src/App.tsx" }. */
export interface Step {
  id: string;
  tool: string;
  target: string;
  status: StepStatus;
}

/** The agent asks to run a command or to edit a file. */
export type PermissionRequest =
  | { sessionId: string; tool: string; command: string }
  | { sessionId: string; tool: string; file: string };

export type PermissionDecision = "allow" | "always" | "deny";

/** The agent asks the user to pick among options. */
export interface QuestionRequest {
  sessionId?: string;
  question: string;
  options: string[];
  multiSelect: boolean;
}

/** Selected options, or "terminal" when the user chose to reply in the terminal instead. */
export type QuestionAnswer = string[] | "terminal";

export type AgentEvent =
  | { type: "session-started"; session: Session }
  | { type: "session-updated"; sessionId: string; changes: Partial<Omit<Session, "id">> }
  | { type: "session-ended"; sessionId: string }
  | { type: "step"; sessionId: string; step: Step }
  | { type: "permission-requested"; request: PermissionRequest }
  | { type: "question-asked"; request: QuestionRequest };
