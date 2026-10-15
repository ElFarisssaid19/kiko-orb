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

/** sending: Kiko has it, the session not yet · delivered: its channel handed it to Claude Code · failed: it never got there. */
export type ChatDelivery = "sending" | "delivered" | "failed";

/** One line of a session's chat: what the user wrote, or Claude's reply. */
export interface ChatEntry {
  id: string;
  from: "you" | "claude";
  text: string;
  /** Epoch ms. */
  at: number;
  /** The user's messages only. */
  delivery?: ChatDelivery;
  /** Why a message failed. */
  error?: string;
}

/** Chat setup as reported by the app, see `chat_status` in src-tauri/src/lib.rs. */
export interface ChatSetup {
  status: "installed" | "not-installed" | "claude-missing";
  /** kiko-channel and Kiko's mcp.json are in place. */
  installed: boolean;
  /** …and the same as this Kiko would install. */
  upToDate: boolean;
  /** Something of Kiko's chat is on disk: it can be removed. */
  removable: boolean;
  claudePath: string | null;
  /** e.g. "2.1.296"; null if `claude --version` didn't answer. */
  claudeVersion: string | null;
  /** Older than the first Claude Code with channels (minClaudeVersion). */
  claudeTooOld: boolean;
  minClaudeVersion: string;
  /** The command that starts a chat session. */
  launchCommand: string;
  /** `alias kiko-claude='…'`, for the user's shell configuration. */
  alias: string;
  binaryPath: string;
  mcpConfigPath: string;
  /** kiko-channel isn't built next to Kiko, and how to build it. */
  sourceError: string | null;
  error: string | null;
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
  /** Chat history per session id, oldest first. In memory only: dropped when the session ends. */
  chats: Record<string, ChatEntry[]>;
  /** Sessions a chat channel serves now. */
  chatLinks: Record<string, true>;
  /** The session whose chat the expanded island shows; null for the overview. */
  chatSessionId: string | null;
  /** Claude's replies not seen yet, per session id. */
  unread: Record<string, number>;
  chatSetup: ChatSetup | null;
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
  chats: {},
  chatLinks: {},
  chatSessionId: null,
  unread: {},
  chatSetup: null,
};
