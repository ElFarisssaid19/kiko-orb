import { create } from "zustand";
import { applyEvent, type KikoEvent } from "../events/applyEvent";
import type { RequestResponse } from "../types/events";
import { initialData, type HooksStatus, type IslandState, type KikoData } from "./state";

export type { HooksStatus, IslandState, KikoData, PendingRequest, Session } from "./state";

/** Sends an answer back to the agent; set by `connectEventSource`. */
type Responder = (requestId: string, response: RequestResponse) => Promise<void>;
let responder: Responder | null = null;

export function setResponder(next: Responder | null) {
  responder = next;
}

interface KikoActions {
  /** Applies an event from the event source (or one of Kiko's own). */
  dispatch: (event: KikoEvent) => void;
  setIslandState: (islandState: IslandState) => void;
  selectSession: (id: string) => void;
  /** Answers a pending request: the island updates at once, then the agent is told. */
  answer: (requestId: string, response: RequestResponse) => void;
  setHooksStatus: (hooksStatus: HooksStatus | null) => void;
}

export type KikoState = KikoData & KikoActions;

export const useKikoStore = create<KikoState>()((set) => ({
  ...initialData,

  dispatch: (event) => set((state) => applyEvent(state, event)),
  // With nothing to show, folding the island goes all the way back to the hidden pill.
  setIslandState: (islandState) =>
    set((state) => ({
      islandState:
        islandState === "compact" && state.sessions.length === 0 && state.pending.length === 0 ? "hidden" : islandState,
    })),
  selectSession: (activeSessionId) => set({ activeSessionId }),
  answer: (requestId, response) => {
    set((state) => applyEvent(state, { type: "answered", requestId, response }));
    responder?.(requestId, response).catch((error) => console.warn("Kiko could not send the answer:", error));
  },
  setHooksStatus: (hooksStatus) => set({ hooksStatus }),
}));
