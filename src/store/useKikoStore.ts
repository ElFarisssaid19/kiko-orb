import { create } from "zustand";
import type {
  PermissionDecision,
  PermissionRequest,
  QuestionAnswer,
  QuestionRequest,
  Session,
  Step,
} from "../types/events";
import type { Mood } from "../types/kiko";

export type IslandState = "hidden" | "compact" | "expanded";

/** Steps kept per session. */
export const MAX_STEPS = 5;

/** What the user answered in the island. A later step sends it back to the agent. */
export type Decision =
  | { kind: "permission"; request: PermissionRequest; decision: PermissionDecision }
  | { kind: "question"; request: QuestionRequest; answer: QuestionAnswer };

interface KikoData {
  islandState: IslandState;
  mood: Mood;
  activeSessions: number;
  sessions: Session[];
  /** Session whose steps the expanded island shows. */
  activeSessionId: string | null;
  /** Last MAX_STEPS steps per session id, oldest first. */
  steps: Record<string, Step[]>;
  pendingPermission: PermissionRequest | null;
  pendingQuestion: QuestionRequest | null;
  lastDecision: Decision | null;
}

interface KikoActions {
  setIslandState: (islandState: IslandState) => void;
  setMood: (mood: Mood) => void;
  setSessions: (sessions: Session[]) => void;
  upsertSession: (session: Session) => void;
  updateSession: (id: string, changes: Partial<Omit<Session, "id">>) => void;
  removeSession: (id: string) => void;
  selectSession: (id: string) => void;
  /** Adds a step, or replaces the one with the same id (e.g. running → done). */
  addStep: (sessionId: string, step: Step) => void;
  setSteps: (steps: Record<string, Step[]>) => void;
  /** The agent waits for approval: Kiko needs you and the island opens on the card. */
  requestPermission: (request: PermissionRequest) => void;
  resolvePermission: (decision: PermissionDecision) => void;
  askQuestion: (request: QuestionRequest) => void;
  answerQuestion: (answer: QuestionAnswer) => void;
  reset: () => void;
}

export type KikoState = KikoData & KikoActions;

const initialData: KikoData = {
  islandState: "hidden",
  mood: "sleeping",
  activeSessions: 0,
  sessions: [],
  activeSessionId: null,
  steps: {},
  pendingPermission: null,
  pendingQuestion: null,
  lastDecision: null,
};

/** Mood when nothing waits for the user. */
const restingMood = (sessions: Session[]): Mood => (sessions.length > 0 ? "working" : "sleeping");

/** Session list changes: keep the derived fields consistent. */
function withSessions(state: KikoData, sessions: Session[]): Partial<KikoData> {
  const waiting = state.pendingPermission !== null || state.pendingQuestion !== null;
  const activeStillThere = sessions.some((s) => s.id === state.activeSessionId);
  return {
    sessions,
    activeSessions: sessions.length,
    activeSessionId: activeStillThere ? state.activeSessionId : (sessions[0]?.id ?? null),
    mood: waiting ? "needsYou" : restingMood(sessions),
  };
}

/** After an answer: show the other pending card if there is one, otherwise settle back to compact. */
function afterAnswer(state: KikoData, remaining: Pick<KikoData, "pendingPermission" | "pendingQuestion">) {
  const next = remaining.pendingPermission ?? remaining.pendingQuestion;
  return {
    ...remaining,
    mood: next ? ("needsYou" as const) : restingMood(state.sessions),
    islandState: next ? ("expanded" as const) : ("compact" as const),
    // The steps shown follow the card on screen.
    activeSessionId: next?.sessionId ?? state.activeSessionId,
  };
}

export const useKikoStore = create<KikoState>()((set) => ({
  ...initialData,

  setIslandState: (islandState) => set({ islandState }),
  setMood: (mood) => set({ mood }),

  setSessions: (sessions) => set((state) => withSessions(state, sessions)),
  upsertSession: (session) =>
    set((state) => {
      const exists = state.sessions.some((s) => s.id === session.id);
      const sessions = exists
        ? state.sessions.map((s) => (s.id === session.id ? session : s))
        : [...state.sessions, session];
      return withSessions(state, sessions);
    }),
  updateSession: (id, changes) =>
    set((state) => ({ sessions: state.sessions.map((s) => (s.id === id ? { ...s, ...changes } : s)) })),
  removeSession: (id) =>
    set((state) => {
      const { [id]: _removed, ...steps } = state.steps;
      return { ...withSessions(state, state.sessions.filter((s) => s.id !== id)), steps };
    }),
  selectSession: (activeSessionId) => set({ activeSessionId }),

  addStep: (sessionId, step) =>
    set((state) => {
      const current = state.steps[sessionId] ?? [];
      const next = current.some((s) => s.id === step.id)
        ? current.map((s) => (s.id === step.id ? step : s))
        : [...current, step].slice(-MAX_STEPS);
      return { steps: { ...state.steps, [sessionId]: next } };
    }),
  setSteps: (steps) =>
    set({ steps: Object.fromEntries(Object.entries(steps).map(([id, list]) => [id, list.slice(-MAX_STEPS)])) }),

  requestPermission: (request) =>
    set({ pendingPermission: request, mood: "needsYou", islandState: "expanded", activeSessionId: request.sessionId }),
  resolvePermission: (decision) =>
    set((state) => {
      if (!state.pendingPermission) return {};
      return {
        lastDecision: { kind: "permission", request: state.pendingPermission, decision },
        ...afterAnswer(state, { pendingPermission: null, pendingQuestion: state.pendingQuestion }),
      };
    }),

  askQuestion: (request) =>
    set((state) => ({
      pendingQuestion: request,
      mood: "needsYou",
      islandState: "expanded",
      // A pending permission is shown first (and keeps its session); the question waits behind it.
      activeSessionId: state.pendingPermission ? state.activeSessionId : (request.sessionId ?? state.activeSessionId),
    })),
  answerQuestion: (answer) =>
    set((state) => {
      if (!state.pendingQuestion) return {};
      return {
        lastDecision: { kind: "question", request: state.pendingQuestion, answer },
        ...afterAnswer(state, { pendingPermission: state.pendingPermission, pendingQuestion: null }),
      };
    }),

  reset: () => set(initialData),
}));
