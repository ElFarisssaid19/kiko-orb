import { useCallback, useEffect, useState } from "react";
import {
  applyHooksChange,
  getHooksStatus,
  onHooksChanged,
  openWindow,
  previewHooksChange,
  type HooksAction,
  type HooksPreview,
} from "../native";
import type { HooksStatus } from "../store/state";
import { BUTTON } from "./buttons";
import { ChatSettings } from "./ChatSettings";

function statusChip(status: HooksStatus): { label: string; dot: string } {
  switch (status.status) {
    case "installed":
      return status.upToDate
        ? { label: "Installed", dot: "bg-done" }
        : { label: "Needs an update", dot: "bg-needs-you" };
    case "not-installed":
      return { label: "Not installed", dot: "bg-text-2" };
    case "port-busy":
      return { label: `Port ${status.port} busy`, dot: "bg-error" };
    case "server-error":
      return { label: "Hook server error", dot: "bg-error" };
  }
}

const fileName = (path: string) => path.split(/[\\/]/).pop() || path;

/** Settings window: install or remove Kiko's Claude Code hooks, after showing exactly what changes. */
export function SettingsPanel() {
  const [status, setStatus] = useState<HooksStatus | null>(null);
  const [preview, setPreview] = useState<HooksPreview | null>(null);
  const [message, setMessage] = useState<{ error: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    getHooksStatus()
      .then(setStatus)
      .catch((error) => setMessage({ error: true, text: String(error) }));
  }, []);

  useEffect(() => {
    refresh();
    let stopped = false;
    let unlisten: (() => void) | undefined;
    void onHooksChanged(refresh).then((stop) => (stopped ? stop() : (unlisten = stop)));
    return () => {
      stopped = true;
      unlisten?.();
    };
  }, [refresh]);

  const review = async (action: HooksAction) => {
    setMessage(null);
    try {
      setPreview(await previewHooksChange(action));
    } catch (error) {
      setMessage({ error: true, text: String(error) });
    }
  };

  const confirm = async () => {
    if (!preview) return;
    setBusy(true);
    try {
      const applied = await applyHooksChange(preview.action, preview.fingerprint);
      const done = preview.action === "install" ? "Hooks installed." : "Hooks removed.";
      setMessage({
        error: false,
        text: applied.backupPath ? `${done} Previous file saved as ${applied.backupPath}` : done,
      });
    } catch (error) {
      setMessage({ error: true, text: String(error) });
    } finally {
      setBusy(false);
      setPreview(null);
      refresh();
    }
  };

  const chip = status && statusChip(status);
  const installed = status?.status === "installed";

  return (
    <main className="h-full overflow-auto bg-bg px-6 py-5 text-text">
      <div className="mx-auto flex max-w-lg flex-col gap-6">
        <h1 className="text-xl font-semibold">Kiko settings</h1>

        <section aria-labelledby="hooks-title" className="flex flex-col gap-3">
          <div className="flex items-center justify-between gap-3">
            <h2 id="hooks-title" className="text-base font-semibold">
              Claude Code hooks
            </h2>
            {chip && (
              <span className="flex items-center gap-2 rounded-full border border-border bg-island px-3 py-1 text-sm">
                <span className={`size-2 rounded-full ${chip.dot}`} />
                {chip.label}
              </span>
            )}
          </div>
          <p className="text-sm text-text-2">
            For each event, Claude Code runs a small curl command that sends it to Kiko on this computer only
            (127.0.0.1:{status?.port ?? "…"}). The secret token stays in ~/.config/kiko/auth-header: settings.json only
            names that file. Permission requests and questions wait up to 110 s for your answer in the island, then
            Claude Code asks in the terminal as usual.
          </p>
          {status?.error && <p className="text-sm text-error">{status.error}</p>}
          {status?.curlError && (
            <p role="alert" className="rounded-lg border border-error/40 bg-island px-3 py-2 text-sm text-error">
              {status.curlError}
            </p>
          )}
          {status && (
            <p className="font-mono text-xs break-all text-text-2">Settings file: {status.settingsPath}</p>
          )}

          {!preview && status && (
            <div className="flex gap-2">
              {(!installed || !status.upToDate) && (
                <button
                  type="button"
                  className={BUTTON.primary}
                  disabled={status.curlError !== null}
                  onClick={() => void review("install")}
                >
                  {installed ? "Update hooks…" : "Install hooks…"}
                </button>
              )}
              {installed && (
                <button type="button" className={BUTTON.secondary} onClick={() => void review("uninstall")}>
                  Remove hooks…
                </button>
              )}
            </div>
          )}

          {preview && (
            <div className="flex flex-col gap-3 rounded-2xl border border-needs-you/40 bg-island p-4">
              <h3 className="text-sm font-semibold">
                {preview.action === "install" ? "Add Kiko's hooks" : "Remove Kiko's hooks"}
              </h3>
              {preview.nothingToDo ? (
                <p className="text-sm text-text-2">Nothing to change: the file is already in that state.</p>
              ) : (
                <p className="text-sm text-text-2">
                  {preview.fileExists
                    ? `Kiko first saves a copy of ${fileName(preview.settingsPath)} as ${fileName(preview.settingsPath)}.kiko-backup-<date>, then `
                    : `${fileName(preview.settingsPath)} doesn't exist yet: Kiko creates it and `}
                  {preview.action === "install"
                    ? `adds the entries below${
                        preview.replacesHttp
                          ? ", replacing its earlier http hooks"
                          : preview.addsProcessId
                            ? ", replacing its earlier ones, which don't send the Claude Code process id that chat needs"
                            : preview.replacesExisting
                              ? ", replacing its earlier ones"
                              : ""
                      }.`
                    : "removes only the entries below."}{" "}
                  Your other settings and hooks stay as they are.
                </p>
              )}
              <pre className="max-h-72 overflow-auto rounded-lg border border-border bg-bg p-3 font-mono text-xs leading-5 text-text">
                {preview.entries}
              </pre>
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
                  Write {fileName(preview.settingsPath)}
                </button>
              </div>
            </div>
          )}

          {message && (
            <p role="status" className={`text-sm break-all ${message.error ? "text-error" : "text-done"}`}>
              {message.text}
            </p>
          )}
        </section>

        <ChatSettings hooksStatus={status} />

        <section aria-labelledby="closed-title" className="flex flex-col gap-2">
          <h2 id="closed-title" className="text-base font-semibold">
            When Kiko is closed
          </h2>
          <p className="text-sm text-text-2">
            Claude Code keeps working normally: each hook command finds Kiko closed, gives up at once and stays silent.
            A chat session keeps running too: its channel waits quietly for Kiko, and Claude answers in the terminal
            instead. Remove the hooks and chat here if you stop using Kiko.
          </p>
        </section>

        {import.meta.env.DEV && (
          <section aria-label="Debug" className="flex gap-2">
            <button type="button" className={BUTTON.secondary} onClick={() => void openWindow("playground")}>
              Open debug playground
            </button>
          </section>
        )}
      </div>
    </main>
  );
}
