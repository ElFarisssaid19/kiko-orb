import { create } from "zustand";
import { applyEvent, type KikoEvent } from "../events/applyEvent";
import type { ChatMessage, RequestResponse } from "../types/events";
import { initialData, type ChatSetup, type HooksStatus, type IslandState, type KikoData } from "./state";

export type { ChatEntry, ChatSetup, HooksStatus, IslandState, KikoData, PendingRequest, Session } from "./state";

/** Sends an answer back to the agent; set by `connectEventSource`. */
type Responder = (requestId: string, response: RequestResponse) => Promise<void>;
let responder: Responder | null = null;

export function setResponder(next: Responder | null) {
  responder = next;
}

/** Sends a chat message to a session's channel; set by `connectEventSource`. */
type ChatSender = (sessionId: string, message: ChatMessage) => Promise<void>;
let chatSender: ChatSender | null = null;

export function setChatSender(next: ChatSender | null) {
  chatSender = next;
}

let sent = 0;
/** Unique within this window; the channel passes it on as the message's `message_id`. */
const newMessageId = () => `m${Date.now().toString(36)}-${++sent}`;

interface KikoActions {
  /** Applies an event from the event source (or one of Kiko's own). */
  dispatch: (event: KikoEvent) => void;
  setIslandState: (islandState: IslandState) => void;
  selectSession: (id: string) => void;
  /** Answers a pending request: the island updates at once, then the agent is told. */
  answer: (requestId: string, response: RequestResponse) => void;
  setHooksStatus: (hooksStatus: HooksStatus | null) => void;
  /** Shows a session's chat in the expanded island. */
  openChat: (sessionId: string) => void;
  closeChat: () => void;
  /** Sends the user's message: it shows at once, then follows its way to the session's channel. */
  sendChat: (sessionId: string, text: string) => void;
  setChatSetup: (chatSetup: ChatSetup | null) => void;
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
  openChat: (sessionId) => set((state) => applyEvent(state, { type: "chat-opened", sessionId })),
  closeChat: () => set((state) => applyEvent(state, { type: "chat-closed" })),
  sendChat: (sessionId, text) => {
    const message: ChatMessage = { id: newMessageId(), text, at: Date.now() };
    const dispatch = (event: KikoEvent) => set((state) => applyEvent(state, event));
    const fail = (error: unknown) =>
      dispatch({
        type: "chat-failed",
        sessionId,
        messageId: message.id,
        error: error instanceof Error ? error.message : String(error),
      });
    dispatch({ type: "chat-sending", sessionId, message });
    if (!chatSender) return fail("Kiko is not connected to Claude Code.");
    chatSender(sessionId, message).catch(fail);
  },
  setChatSetup: (chatSetup) => set({ chatSetup }),
}));
