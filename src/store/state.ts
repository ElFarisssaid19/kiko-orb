import type { AgentKind, PermissionRequest, QuestionRequest, RequestResponse, Step } from "../types/events";
import type { Mood } from "../types/kiko";

export type IslandState = "hidden" | "compact" | "expanded";

/**
 * idle: open, between turns · working: in a turn · waiting: blocked on a prompt in the terminal ·
 * done: the turn just ended (4 s) · error: the turn ended on an API error.
 */
export type SessionStatus = "idle" | "working" | "waiting" | "done" | "error";

export interface Session {
  id: string;
  project: string;
  cwd: string;
  agent: AgentKind;
  status: SessionStatus;
  /** Human-readable current activity, e.g. "Editing app.tsx". */
  currentStep: string;
  /** Epoch ms of the first event Kiko saw from this session. */
  startedAt: number;
  linesAdded: number;
  linesRemoved: number;
}

/** A permission or question waiting for the user. */
export type PendingRequest =
  | { kind: "permission"; request: PermissionRequest }
  | { kind: "question"; request: QuestionRequest };

/** The user's last answer in the island. */
export interface Decision {
  requestId: string;
  response: RequestResponse;
}

/** "installed" … as reported by the app, see `hooks_status` in src-tauri/src/lib.rs. */
export interface HooksStatus {
  status: "installed" | "not-installed" | "port-busy" | "server-error";
  /** Installed with the current hook commands (port, header file). */
  upToDate: boolean;
  port: number;
  settingsPath: string;
  error: string | null;
  /** The hooks run curl: why it can't be used here, with the command to fix it. */
  curlError: string | null;
}

/** Steps kept per session. */
export const MAX_STEPS = 5;

export interface KikoData {
  islandState: IslandState;
  mood: Mood;
  /** Sessions in a turn right now: the satellite speeds up with them. */
  activeSessions: number;
  sessions: Session[];
  /** Session whose steps the expanded island shows. */
  activeSessionId: string | null;
  /** Last MAX_STEPS steps per session id, oldest first. */
  steps: Record<string, Step[]>;
  /** Oldest first: the island shows the first one. */
  pending: PendingRequest[];
  lastDecision: Decision | null;
  hooksStatus: HooksStatus | null;
}

export const initialData: KikoData = {
  islandState: "hidden",
  mood: "sleeping",
  activeSessions: 0,
  sessions: [],
  activeSessionId: null,
  steps: {},
  pending: [],
  lastDecision: null,
  hooksStatus: null,
};
