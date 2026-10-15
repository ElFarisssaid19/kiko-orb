// Typed wrappers around Kiko's own Tauri commands (src-tauri/src/lib.rs), other than events.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ChatSetup, HooksStatus } from "./store/state";

export type HooksAction = "install" | "uninstall";

/** What a hooks change will write, shown before confirming. */
export interface HooksPreview {
  action: HooksAction;
  settingsPath: string;
  fileExists: boolean;
  /** The `hooks` entries added (install) or removed (uninstall), as JSON. */
  entries: string;
  replacesExisting: boolean;
  /** The earlier install used http hooks, from before Kiko switched to curl commands. */
  replacesHttp: boolean;
  /** The earlier install's commands don't send the Claude Code process id, which chat needs. */
  addsProcessId: boolean;
  nothingToDo: boolean;
  fingerprint: string;
}

export interface HooksApplied {
  changed: boolean;
  backupPath: string | null;
}

export type Page = "island" | "settings" | "playground";

/** Which page this window shows: Tauri windows by label, a browser tab by `?settings` / `?playground`. */
export function currentPage(): Page {
  const params = new URLSearchParams(window.location.search);
  const label = isTauri() ? getCurrentWindow().label : "main";
  if (label === "settings" || params.has("settings")) return "settings";
  if (import.meta.env.DEV && (label === "playground" || params.has("playground"))) return "playground";
  return "island";
}

export function getHooksStatus(): Promise<HooksStatus | null> {
  return isTauri() ? invoke<HooksStatus>("hooks_status") : Promise.resolve(null);
}

/** Calls `onChange` after Kiko's hooks are installed or removed, from any window. */
export async function onHooksChanged(onChange: () => void): Promise<() => void> {
  return isTauri() ? listen("kiko://hooks-changed", onChange) : () => {};
}

export function previewHooksChange(action: HooksAction): Promise<HooksPreview> {
  return invoke<HooksPreview>("preview_hooks_change", { action });
}

export function applyHooksChange(action: HooksAction, fingerprint: string): Promise<HooksApplied> {
  return invoke<HooksApplied>("apply_hooks_change", { action, fingerprint });
}

/** One thing a chat install or removal does, shown before it happens. */
export interface ChatStep {
  kind: "create-dir" | "copy" | "write" | "remove" | "remove-dir";
  /** With `~` for the home directory. */
  path: string;
  detail: string;
  /** The text written, for "write". */
  content: string | null;
}

export interface ChatPreview {
  action: HooksAction;
  steps: ChatStep[];
  nothingToDo: boolean;
  fingerprint: string;
}

export function getChatSetup(): Promise<ChatSetup | null> {
  return isTauri() ? invoke<ChatSetup>("chat_status") : Promise.resolve(null);
}

/** Calls `onChange` after Kiko's chat is installed or removed, from any window. */
export async function onChatChanged(onChange: () => void): Promise<() => void> {
  return isTauri() ? listen("kiko://chat-changed", onChange) : () => {};
}

export function previewChatChange(action: HooksAction): Promise<ChatPreview> {
  return invoke<ChatPreview>("preview_chat_change", { action });
}

export function applyChatChange(action: HooksAction, fingerprint: string): Promise<void> {
  return invoke("apply_chat_change", { action, fingerprint });
}

export function openWindow(kind: "settings" | "playground"): Promise<void> {
  return isTauri() ? invoke("open_window", { kind }) : Promise.resolve();
}
