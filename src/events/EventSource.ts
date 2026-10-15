import type { AgentEvent, ChatMessage, HookLogEntry, RequestResponse } from "../types/events";

export type AgentEventListener = (event: AgentEvent) => void;
export type HookLogListener = (log: HookLogEntry[]) => void;
export type Unsubscribe = () => void;

/**
 * The only way the UI receives agent events and answers them. Implementation: ClaudeCodeSource,
 * backed by Claude Code hooks and Kiko's chat channels.
 *
 * Note: this shadows the DOM's global `EventSource` (SSE) type — always import it explicitly.
 */
export interface EventSource {
  start(): Promise<void> | void;
  stop(): void;
  subscribe(listener: AgentEventListener): Unsubscribe;
  /** Sends the user's answer to a pending permission or question back to the agent. */
  respond(requestId: string, response: RequestResponse): Promise<void>;
  /**
   * Sends the user's chat message to the session's channel. Resolves once Kiko holds it; a
   * `chat-delivered` event follows when the session has it. Rejects when the session has no channel.
   */
  sendChat(sessionId: string, message: ChatMessage): Promise<void>;
  /** Raw hook traffic for debugging: called with the latest entries (newest last) on every change. */
  subscribeLog(listener: HookLogListener): Unsubscribe;
}
