import type { AgentEvent, HookLogEntry, RequestResponse } from "../types/events";

export type AgentEventListener = (event: AgentEvent) => void;
export type HookLogListener = (log: HookLogEntry[]) => void;
export type Unsubscribe = () => void;

/**
 * The only way the UI receives agent events and answers them. Implementation: ClaudeCodeSource,
 * backed by Claude Code hooks.
 *
 * Note: this shadows the DOM's global `EventSource` (SSE) type — always import it explicitly.
 */
export interface EventSource {
  start(): Promise<void> | void;
  stop(): void;
  subscribe(listener: AgentEventListener): Unsubscribe;
  /** Sends the user's answer to a pending permission or question back to the agent. */
  respond(requestId: string, response: RequestResponse): Promise<void>;
  /** Raw hook traffic for debugging: called with the latest entries (newest last) on every change. */
  subscribeLog(listener: HookLogListener): Unsubscribe;
}
