import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AgentEvent, HookLogEntry, RequestResponse } from "../types/events";
import type { AgentEventListener, EventSource, HookLogListener, Unsubscribe } from "./EventSource";

/** Tauri events sent by the hook server (src-tauri/src/lib.rs). */
const AGENT_EVENT = "kiko://agent-event";
const HOOK_LOG = "kiko://hook-log";
const LOG_SIZE = 50;

/** Real Claude Code sessions, through Kiko's local hook server. */
export class ClaudeCodeSource implements EventSource {
  private listeners = new Set<AgentEventListener>();
  private logListeners = new Set<HookLogListener>();
  private log: HookLogEntry[] = [];
  private unlisten: UnlistenFn[] = [];

  async start(): Promise<void> {
    // A plain browser tab (npm run dev) has no hook server behind it.
    if (!isTauri()) return;
    this.unlisten.push(await listen<AgentEvent>(AGENT_EVENT, ({ payload }) => this.emit(payload)));
    this.unlisten.push(await listen<HookLogEntry>(HOOK_LOG, ({ payload }) => this.setLog([...this.log, payload])));
    // Catch up with what happened before this window opened: waiting requests and recent hook traffic.
    const [pending, log] = await Promise.all([
      invoke<AgentEvent[]>("pending_requests"),
      invoke<HookLogEntry[]>("hook_log"),
    ]);
    pending.forEach((event) => this.emit(event));
    this.setLog(log);
  }

  stop(): void {
    this.unlisten.forEach((unlisten) => unlisten());
    this.unlisten = [];
  }

  subscribe(listener: AgentEventListener): Unsubscribe {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  respond(requestId: string, response: RequestResponse): Promise<void> {
    return invoke("respond_to_request", { requestId, response });
  }

  subscribeLog(listener: HookLogListener): Unsubscribe {
    this.logListeners.add(listener);
    listener(this.log);
    return () => {
      this.logListeners.delete(listener);
    };
  }

  private emit(event: AgentEvent) {
    this.listeners.forEach((listener) => listener(event));
  }

  private setLog(log: HookLogEntry[]) {
    this.log = log.slice(-LOG_SIZE);
    this.logListeners.forEach((listener) => listener(this.log));
  }
}
