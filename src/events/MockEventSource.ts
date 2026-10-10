import type { KikoEvent } from "../types/events";
import type { EventSource, KikoEventListener, Unsubscribe } from "./EventSource";

/** Placeholder: emits nothing yet. Scripted mock events come in a later step. */
export class MockEventSource implements EventSource {
  private listeners = new Set<KikoEventListener>();

  start(): void {}

  stop(): void {}

  subscribe(listener: KikoEventListener): Unsubscribe {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  protected emit(event: KikoEvent): void {
    this.listeners.forEach((listener) => listener(event));
  }
}
