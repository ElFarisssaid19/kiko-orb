import type { AgentEvent } from "../types/events";
import type { EventSource, AgentEventListener, Unsubscribe } from "./EventSource";

/** Placeholder: emits nothing yet. Scripted mock events come in a later step. */
export class MockEventSource implements EventSource {
  private listeners = new Set<AgentEventListener>();

  start(): void {}

  stop(): void {}

  subscribe(listener: AgentEventListener): Unsubscribe {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  protected emit(event: AgentEvent): void {
    this.listeners.forEach((listener) => listener(event));
  }
}
