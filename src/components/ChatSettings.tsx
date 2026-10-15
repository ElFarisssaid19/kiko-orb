import { useCallback, useEffect, useState } from "react";
import {
  applyChatChange,
  getChatSetup,
  onChatChanged,
  previewChatChange,
  type ChatPreview,
  type ChatStep,
  type HooksAction,
} from "../native";
import type { ChatSetup, HooksStatus } from "../store/state";
import { BUTTON } from "./buttons";
import { CopyButton } from "./CopyButton";

function statusChip(setup: ChatSetup): { label: string; dot: string } {
  switch (setup.status) {
    case "installed":
      return setup.upToDate
        ? { label: "Installed", dot: "bg-done" }
        : { label: "Needs an update", dot: "bg-needs-you" };
    case "not-installed":
      return { label: "Not installed", dot: "bg-text-2" };
    case "claude-missing":
      return { label: "Claude CLI not found", dot: "bg-error" };
  }
}

const STEP_LABELS: Record<ChatStep["kind"], string> = {
  "create-dir": "Create folder",
  copy: "Copy",
  write: "Write",
  remove: "Remove",
  "remove-dir": "Remove folder",
};

/** A command to copy: the text, selectable, and a Copy button. */
function CommandLine({ text }: { text: string }) {
  return (
    <div className="flex items-start gap-2">
      <code className="min-w-0 flex-1 rounded-lg border border-border bg-island px-3 py-2 font-mono text-xs leading-5 break-all text-text select-text">
        {text}
      </code>
      <CopyButton text={text} />
    </div>
  );
}

/**
 * Settings → Chat: installs kiko-channel and Kiko's mcp.json for the sessions started with the launch
 * command, after showing every file it creates or removes. Nothing else is touched: no command is
 * run, and the user adds the suggested alias to their shell themselves.
 */
export function ChatSettings({ hooksStatus }: { hooksStatus: HooksStatus | null }) {
  const [setup, setSetup] = useState<ChatSetup | null>(null);
  const [preview, setPreview] = useState<ChatPreview | null>(null);
  const [message, setMessage] = useState<{ error: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    getChatSetup()
      .then(setSetup)
      .catch((error) => setMessage({ error: true, text: String(error) }));
  }, []);

  useEffect(() => {
    refresh();
    let stopped = false;
    let unlisten: (() => void) | undefined;
    void onChatChanged(refresh).then((stop) => (stopped ? stop() : (unlisten = stop)));
    return () => {
      stopped = true;
      unlisten?.();
    };
  }, [refresh]);

  const review = async (action: HooksAction) => {
    setMessage(null);
    try {
      setPreview(await previewChatChange(action));
    } catch (error) {
      setMessage({ error: true, text: String(error) });
    }
  };

  const confirm = async () => {
    if (!preview) return;
    setBusy(true);
    try {
      await applyChatChange(preview.action, preview.fingerprint);
      setMessage({
        error: false,
        text: preview.action === "install" ? "Chat installed." : "Chat removed.",
      });
    } catch (error) {
      setMessage({ error: true, text: String(error) });
    } finally {
      setBusy(false);
      setPreview(null);
      refresh();
    }
  };

  const chip = setup && statusChip(setup);
  const hooksReady = hooksStatus?.status === "installed" && hooksStatus.upToDate;

  return (
    <section aria-labelledby="chat-title" className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <h2 id="chat-title" className="text-base font-semibold">
          Chat with sessions
        </h2>
        {chip && (
          <span className="flex items-center gap-2 rounded-full border border-border bg-island px-3 py-1 text-sm">
            <span className={`size-2 rounded-full ${chip.dot}`} />
            {chip.label}
          </span>
        )}
      </div>
      <p className="text-sm text-text-2">
        Write to a running Claude Code session from the island and read Claude's replies there. Kiko uses Claude Code
        channels (a research preview that needs a claude.ai login). Install copies a small program, kiko-channel, and
        writes Kiko's own MCP config. Only the sessions you start with the launch command below load it: your other
        sessions never start it, and ~/.claude.json and settings.json are not touched.
      </p>

      {setup && (
        <p className="font-mono text-xs text-text-2">
          {setup.claudeVersion
            ? `Claude Code ${setup.claudeVersion}`
            : setup.claudePath
              ? "Claude Code (its version did not answer)"
              : "Claude Code: not found"}
          {setup.claudePath && ` · ${setup.claudePath}`}
        </p>
      )}
      {setup?.status === "claude-missing" && (
        <p role="alert" className="rounded-lg border border-error/40 bg-island px-3 py-2 text-sm text-error">
          The claude command was not found on this computer. Install Claude Code first, then come back here.
        </p>
      )}
      {setup?.claudeTooOld && (
        <p role="alert" className="rounded-lg border border-needs-you/40 bg-island px-3 py-2 text-sm text-needs-you">
          Chat needs Claude Code {setup.minClaudeVersion} or newer, and this one is {setup.claudeVersion}. Update it with:{" "}
          <code className="font-mono">claude update</code>
        </p>
      )}
      {setup && !hooksReady && (
        <p className="rounded-lg border border-needs-you/40 bg-island px-3 py-2 text-sm text-text">
          Kiko links each chat to its session through the hooks, which must send the Claude Code process id.{" "}
          {hooksStatus?.status === "installed" ? "Update the hooks above" : "Install the hooks above"} so the chat
          follows /clear and /resume.
        </p>
      )}
      {setup?.sourceError && (!setup.installed || !setup.upToDate) && (
        <p className="rounded-lg border border-error/40 bg-island px-3 py-2 text-sm break-all text-error">
          {setup.sourceError}
        </p>
      )}
      {setup?.error && <p className="text-sm text-error">{setup.error}</p>}

      {!preview && setup && (
        <div className="flex gap-2">
          {(!setup.installed || !setup.upToDate) && (
            <button
              type="button"
              className={BUTTON.primary}
              disabled={setup.sourceError !== null || setup.status === "claude-missing"}
              onClick={() => void review("install")}
            >
              {setup.installed ? "Update chat…" : "Install chat…"}
            </button>
          )}
          {setup.removable && (
            <button type="button" className={BUTTON.secondary} onClick={() => void review("uninstall")}>
              Remove chat…
            </button>
          )}
        </div>
      )}

      {preview && (
        <div className="flex flex-col gap-3 rounded-2xl border border-needs-you/40 bg-island p-4">
          <h3 className="text-sm font-semibold">
            {preview.action === "install" ? "Install Kiko's chat" : "Remove Kiko's chat"}
          </h3>
          {preview.nothingToDo ? (
            <p className="text-sm text-text-2">Nothing to change: everything is already in that state.</p>
          ) : (
            <p className="text-sm text-text-2">
              Kiko will do exactly this, in this order. No command is run, and Claude Code's own files stay as they are.
            </p>
          )}
          <ol className="flex flex-col gap-2.5">
            {preview.steps.map((step, i) => (
              <li key={i} className="flex flex-col gap-1">
                <p className="text-sm text-text">
                  <span className="mr-2 font-mono text-[11px] text-needs-you uppercase">{STEP_LABELS[step.kind]}</span>
                  <span className="font-mono text-xs break-all">{step.path}</span>
                </p>
                <p className="text-xs text-text-2">{step.detail}</p>
                {step.content && (
                  <pre className="max-h-56 overflow-auto rounded-lg border border-border bg-bg p-3 font-mono text-xs leading-5 text-text select-text">
                    {step.content}
                  </pre>
                )}
              </li>
            ))}
          </ol>
          <div className="flex justify-end gap-2">
            <button type="button" className={BUTTON.ghost} onClick={() => setPreview(null)} disabled={busy}>
              Cancel
            </button>
            <button
              type="button"
              className={BUTTON.primary}
              onClick={() => void confirm()}
              disabled={busy || preview.nothingToDo}
            >
              {preview.action === "install" ? "Install" : "Remove"}
            </button>
          </div>
        </div>
      )}

      {message && (
        <p role="status" className={`text-sm break-all ${message.error ? "text-error" : "text-done"}`}>
          {message.text}
        </p>
      )}

      {setup?.installed && (
        <div className="flex flex-col gap-3">
          <div className="flex flex-col gap-1.5">
            <h3 className="text-sm font-semibold">Start a chat session</h3>
            <CommandLine text={setup.launchCommand} />
            <p className="text-xs text-text-2">
              Each time, Claude Code first warns about development channels: choose "I am using this for local
              development". The channel only takes messages from this Kiko (same secret token as the hooks).
            </p>
          </div>
          <div className="flex flex-col gap-1.5">
            <h3 className="text-sm font-semibold">Suggested alias</h3>
            <CommandLine text={setup.alias} />
            <p className="text-xs text-text-2">
              Kiko never edits your shell files: add this line to ~/.bashrc or ~/.zshrc yourself, open a new terminal,
              then run kiko-claude in any project.
            </p>
          </div>
        </div>
      )}
    </section>
  );
}
