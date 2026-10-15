import type { ChatEntry, KikoData } from "../store/state";
import type { ChatMessage } from "../types/events";

/** Shown on the user's messages still on their way when their session's channel goes away. */
export const CHANNEL_CLOSED = "The session's chat channel closed before Claude Code got it.";

function updateChat(state: KikoData, sessionId: string, update: (entries: ChatEntry[]) => ChatEntry[]): KikoData {
  return { ...state, chats: { ...state.chats, [sessionId]: update(state.chats[sessionId] ?? []) } };
}

/** The user's message, shown at once while Kiko hands it to the session's channel. */
export function chatSending(state: KikoData, sessionId: string, message: ChatMessage): KikoData {
  return updateChat(state, sessionId, (entries) => [...entries, { ...message, from: "you", delivery: "sending" }]);
}

export function chatDelivered(state: KikoData, sessionId: string, messageIds: string[]): KikoData {
  const ids = new Set(messageIds);
  return updateChat(state, sessionId, (entries) =>
    entries.map((e) => (e.from === "you" && ids.has(e.id) ? { ...e, delivery: "delivered" } : e)),
  );
}

export function chatFailed(state: KikoData, sessionId: string, messageId: string, error: string): KikoData {
  return updateChat(state, sessionId, (entries) =>
    entries.map((e) => (e.id === messageId && e.delivery !== "delivered" ? { ...e, delivery: "failed", error } : e)),
  );
}

/** The chat the user is looking at right now: replies there are seen as they arrive. */
const isOnScreen = (state: KikoData, sessionId: string) =>
  state.islandState === "expanded" && state.chatSessionId === sessionId && state.pending.length === 0;

export function chatReply(state: KikoData, sessionId: string, message: ChatMessage): KikoData {
  const next = updateChat(state, sessionId, (entries) =>
    // A window that catches up never shows a reply twice.
    entries.some((e) => e.from === "claude" && e.id === message.id) ? entries : [...entries, { ...message, from: "claude" }],
  );
  if (isOnScreen(state, sessionId)) return next;
  return { ...next, unread: { ...next.unread, [sessionId]: (next.unread[sessionId] ?? 0) + 1 } };
}

export function chatLinked(state: KikoData, sessionId: string): KikoData {
  return { ...state, chatLinks: { ...state.chatLinks, [sessionId]: true } };
}

/** The channel went away: messages still on their way never arrive. */
export function chatUnlinked(state: KikoData, sessionId: string): KikoData {
  const { [sessionId]: _gone, ...chatLinks } = state.chatLinks;
  const failed = updateChat(state, sessionId, (entries) =>
    entries.map((e) => (e.delivery === "sending" ? { ...e, delivery: "failed", error: CHANNEL_CLOSED } : e)),
  );
  return { ...(state.chats[sessionId] ? failed : state), chatLinks };
}

/** Opens a session's chat in the expanded island; its replies count as seen. */
export function chatOpened(state: KikoData, sessionId: string): KikoData {
  const { [sessionId]: _seen, ...unread } = state.unread;
  return { ...state, chatSessionId: sessionId, activeSessionId: sessionId, islandState: "expanded", unread };
}

/** History lives in memory only and goes with its session. */
export function withoutChat(state: KikoData, sessionId: string): KikoData {
  const { [sessionId]: _chat, ...chats } = state.chats;
  const { [sessionId]: _link, ...chatLinks } = state.chatLinks;
  const { [sessionId]: _unread, ...unread } = state.unread;
  return {
    ...state,
    chats,
    chatLinks,
    unread,
    chatSessionId: state.chatSessionId === sessionId ? null : state.chatSessionId,
  };
}
