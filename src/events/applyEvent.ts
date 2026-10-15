import { MAX_STEPS, type KikoData, type PendingRequest, type Session, type SessionStatus } from "../store/state";
import type { AgentEvent, ChatMessage, RequestResponse, SessionRef, Step } from "../types/events";
import type { Mood } from "../types/kiko";
import {
  chatDelivered,
  chatFailed,
  chatLinked,
  chatOpened,
  chatReply,
  chatSending,
  chatUnlinked,
  withoutChat,
} from "./chat";

/** Events Kiko produces itself, next to the agent's. */
export type LocalEvent =
  /** 4 s after a turn ended: "done" settles to idle. */
  | { type: "done-expired"; sessionId: string }
  /** The user answered a pending request in the island. */
  | { type: "answered"; requestId: string; response: RequestResponse }
  /** The expanded island shows this session's chat (its replies count as seen). */
  | { type: "chat-opened"; sessionId: string }
  /** Back to the overview. */
  | { type: "chat-closed" }
  /** The user sent a message: shown at once, on its way to the session's channel. */
  | { type: "chat-sending"; sessionId: string; message: ChatMessage }
  /** Kiko couldn't hand the message to the session's channel. */
  | { type: "chat-failed"; sessionId: string; messageId: string; error: string };

export type KikoEvent = AgentEvent | LocalEvent;

/** Notifications that mean Claude Code is waiting for the user in the terminal. */
const WAITING_NOTIFICATIONS = new Set([
  "permission_prompt",
  "elicitation_dialog",
  "elicitation_url_dialog",
  "agent_needs_input",
]);

const NEXT_PROMPT = "Waiting for your next prompt";

const fileName = (path: string) => path.split(/[\\/]/).pop() || path;

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

/** One-line description of a step, e.g. "Editing app.tsx". */
export function describeStep(step: Step): string {
  switch (step.tool) {
    case "Edit":
    case "MultiEdit":
    case "NotebookEdit":
      return `Editing ${fileName(step.target)}`;
    case "Write":
      return `Writing ${fileName(step.target)}`;
    case "Read":
      return `Reading ${fileName(step.target)}`;
    case "Bash":
    case "PowerShell":
      return `Running ${step.target}`;
    case "Grep":
      return `Searching for ${step.target}`;
    case "Glob":
      return `Finding ${step.target}`;
    case "WebFetch":
      return `Reading ${hostOf(step.target)}`;
    case "WebSearch":
      return "Searching the web";
    case "Agent":
    case "Task":
      return "Working with a subagent";
    case "AskUserQuestion":
      return "Asking you a question";
    default:
      return step.target ? `${step.tool}: ${step.target}` : `Using ${step.tool}`;
  }
}

/** Kiko's mood follows the most urgent session state. */
export function moodOf(sessions: Session[], pending: PendingRequest[]): Mood {
  const any = (status: SessionStatus) => sessions.some((s) => s.status === status);
  if (pending.length > 0 || any("waiting")) return "needsYou";
  if (any("error")) return "error";
  if (any("working")) return "working";
  if (any("done")) return "done";
  return sessions.length > 0 ? "idle" : "sleeping";
}

/** Updates the session `ref` points to, creating it if Kiko hasn't seen it yet (Kiko started mid-session). */
function withSession(
  state: KikoData,
  ref: SessionRef,
  at: number,
  update: (session: Session) => Partial<Session>,
): KikoData {
  const existing = state.sessions.find((s) => s.id === ref.id);
  const base: Session = existing ?? {
    id: ref.id,
    project: ref.project,
    cwd: ref.cwd,
    agent: "claude-code",
    status: "idle",
    currentStep: "",
    startedAt: at,
    linesAdded: 0,
    linesRemoved: 0,
  };
  const updated = { ...base, ...update(base) };
  const sessions = existing
    ? state.sessions.map((s) => (s.id === ref.id ? updated : s))
    : [...state.sessions, updated];
  return { ...state, sessions };
}

/** Adds a step, or replaces the one with the same id (running → done); keeps the last MAX_STEPS. */
function withStep(state: KikoData, sessionId: string, step: Step): KikoData {
  const current = state.steps[sessionId] ?? [];
  const next = current.some((s) => s.id === step.id)
    ? current.map((s) => (s.id === step.id ? step : s))
    : [...current, step].slice(-MAX_STEPS);
  return { ...state, steps: { ...state.steps, [sessionId]: next } };
}

function withPending(state: KikoData, request: PendingRequest): KikoData {
  // A window that reloads gets waiting requests again: never list one twice.
  if (state.pending.some((p) => p.request.id === request.request.id)) return state;
  return { ...state, pending: [...state.pending, request] };
}

const withoutPending = (state: KikoData, keep: (p: PendingRequest) => boolean): KikoData => ({
  ...state,
  pending: state.pending.filter(keep),
});

/** The event's direct effect on sessions, steps and the pending queue. */
function reduce(state: KikoData, event: KikoEvent): KikoData {
  switch (event.type) {
    case "session-started":
      return withSession(state, event.session, event.at, () => ({ status: "idle", currentStep: NEXT_PROMPT }));

    case "session-ended": {
      const id = event.session.id;
      const { [id]: _gone, ...steps } = state.steps;
      return {
        ...withoutChat(withoutPending(state, (p) => p.request.sessionId !== id), id),
        sessions: state.sessions.filter((s) => s.id !== id),
        steps,
      };
    }

    case "prompt-submitted":
      return withSession(state, event.session, event.at, () => ({ status: "working", currentStep: "Thinking…" }));

    case "tool-started":
      return withStep(
        withSession(state, event.session, event.at, () => ({
          status: "working",
          currentStep: describeStep(event.step),
        })),
        event.session.id,
        event.step,
      );

    case "tool-finished":
      return withStep(
        withSession(state, event.session, event.at, (s) => ({
          status: "working",
          linesAdded: s.linesAdded + event.linesAdded,
          linesRemoved: s.linesRemoved + event.linesRemoved,
        })),
        event.session.id,
        event.step,
      );

    case "permission-requested":
      return withPending(
        withSession(state, event.session, event.at, () => ({ currentStep: "Waiting for your approval" })),
        { kind: "permission", request: event.request },
      );

    case "question-asked":
      return withPending(
        withSession(state, event.session, event.at, () => ({ currentStep: "Asking you a question" })),
        { kind: "question", request: event.request },
      );

    case "request-resolved":
      return withoutPending(state, (p) => p.request.id !== event.requestId);

    case "notification":
      if (WAITING_NOTIFICATIONS.has(event.notificationType)) {
        return withSession(state, event.session, event.at, () => ({
          status: "waiting",
          currentStep: "Waiting for you in the terminal",
        }));
      }
      if (event.notificationType === "idle_prompt") {
        return withSession(state, event.session, event.at, () => ({ status: "idle", currentStep: NEXT_PROMPT }));
      }
      return withSession(state, event.session, event.at, () => ({}));

    case "turn-stopped":
      return withoutPending(
        withSession(state, event.session, event.at, () => ({ status: "done", currentStep: "Done" })),
        (p) => p.request.sessionId !== event.session.id,
      );

    case "turn-failed":
      return withoutPending(
        withSession(state, event.session, event.at, () => ({
          status: "error",
          currentStep: `API error: ${event.error}`,
        })),
        (p) => p.request.sessionId !== event.session.id,
      );

    case "done-expired":
      return {
        ...state,
        sessions: state.sessions.map((s) =>
          s.id === event.sessionId && s.status === "done" ? { ...s, status: "idle", currentStep: NEXT_PROMPT } : s,
        ),
      };

    case "answered":
      return {
        ...withoutPending(state, (p) => p.request.id !== event.requestId),
        lastDecision: { requestId: event.requestId, response: event.response },
      };

    case "chat-linked":
      return chatLinked(state, event.sessionId);
    case "chat-unlinked":
      return chatUnlinked(state, event.sessionId);
    case "chat-delivered":
      return chatDelivered(state, event.sessionId, event.messageIds);
    case "chat-reply":
      return chatReply(state, event.sessionId, event.message);
    case "chat-opened":
      return chatOpened(state, event.sessionId);
    case "chat-closed":
      return { ...state, chatSessionId: null };
    case "chat-sending":
      return chatSending(state, event.sessionId, event.message);
    case "chat-failed":
      return chatFailed(state, event.sessionId, event.messageId, event.error);
  }
}

/** Everything derived from sessions and requests: mood, island state, active session. */
function settle(previous: KikoData, next: KikoData): KikoData {
  let islandState = next.islandState;
  if (next.sessions.length === 0 && next.pending.length === 0) islandState = "hidden";
  // A new request opens the island on its card.
  else if (next.pending.length > previous.pending.length) islandState = "expanded";
  // The last request went away: back to the pill.
  else if (previous.pending.length > 0 && next.pending.length === 0 && islandState === "expanded") islandState = "compact";
  else if (islandState === "hidden") islandState = "compact";

  const head = next.pending[0]?.request.sessionId;
  const keep = next.sessions.some((s) => s.id === next.activeSessionId) ? next.activeSessionId : null;
  const chatSessionId = next.sessions.some((s) => s.id === next.chatSessionId) ? next.chatSessionId : null;

  return {
    ...next,
    islandState,
    chatSessionId,
    mood: moodOf(next.sessions, next.pending),
    activeSessions: next.sessions.filter((s) => s.status === "working").length,
    activeSessionId: head ?? keep ?? next.sessions[0]?.id ?? null,
  };
}

/** Pure reducer: the store's next state after `event`. */
export function applyEvent(state: KikoData, event: KikoEvent): KikoData {
  return settle(state, reduce(state, event));
}
