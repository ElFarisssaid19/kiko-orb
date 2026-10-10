import type { KikoEvent } from "../types/events";

export type KikoEventListener = (event: KikoEvent) => void;
export type Unsubscribe = () => void;

/**
 * The only way the UI receives agent events. Implementations: MockEventSource (dev),
 * and later a real source backed by Claude Code hooks.
 *
 * Note: this shadows the DOM's global `EventSource` (SSE) type — always import it explicitly.
 */
export interface EventSource {
  start(): void;
  stop(): void;
  subscribe(listener: KikoEventListener): Unsubscribe;
}
