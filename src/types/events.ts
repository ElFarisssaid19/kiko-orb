// Placeholder event model. The real Claude Code event shapes are defined in a later step.

/** Visual state of an agent session. Each value maps to a color token in tokens.css. */
export type AgentStatus = "idle" | "working" | "needs-you" | "done" | "error";

export interface KikoEvent {
  sessionId: string;
  status: AgentStatus;
  timestamp: number;
}
