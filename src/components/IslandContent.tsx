import { useMemo } from "react";
import { useNow } from "../hooks/useNow";
import { openWindow } from "../native";
import { useKikoStore, type HooksStatus, type KikoState, type Session } from "../store/useKikoStore";
import type { Step, StepStatus } from "../types/events";
import { BUTTON } from "./buttons";
import { Kiko } from "./Kiko";
import { PermissionCard } from "./PermissionCard";
import { QuestionCard } from "./QuestionCard";

const FOCUS_INSET = "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-text";

function agentsRunning(count: number): string {
  if (count === 0) return "No agents running";
  return `${count} ${count === 1 ? "agent" : "agents"} running`;
}

const hooksReady = (status: HooksStatus | null) => !status || (status.status === "installed" && status.upToDate);

/** One-line status of the compact island. */
function statusLine(s: KikoState): string {
  const head = s.pending[0];
  if (head?.kind === "permission") return "Waiting for your approval";
  if (head?.kind === "question") return "Claude Code has a question";
  if (s.sessions.length === 0) return hooksReady(s.hooksStatus) ? "No agents running" : "Set up Claude Code hooks";
  if (s.sessions.length === 1) return s.sessions[0].currentStep || s.sessions[0].project;
  return agentsRunning(s.sessions.length);
}

function formatElapsed(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m`;
}

/** +N −M, or nothing when there is no diff. */
function Diff({ sessions }: { sessions: Session[] }) {
  const added = sessions.reduce((sum, s) => sum + s.linesAdded, 0);
  const removed = sessions.reduce((sum, s) => sum + s.linesRemoved, 0);
  if (added === 0 && removed === 0) return null;
  return (
    <span className="shrink-0 font-mono text-[13px]" aria-label={`${added} lines added, ${removed} removed`}>
      <span className="text-done">+{added}</span> <span className="text-error">−{removed}</span>
    </span>
  );
}

/** Compact pill: mini Kiko, status line and diff. Clicking it expands the island. */
export function CompactContent({ onExpand }: { onExpand: () => void }) {
  const mood = useKikoStore((s) => s.mood);
  const activeSessions = useKikoStore((s) => s.activeSessions);
  const sessions = useKikoStore((s) => s.sessions);
  const status = useKikoStore(statusLine);

  return (
    <button
      type="button"
      onClick={onExpand}
      aria-expanded={false}
      className={`flex h-[42px] w-full items-center gap-2.5 rounded-[21px] px-3 text-left ${FOCUS_INSET}`}
    >
      <Kiko mood={mood} size={28} activeSessions={activeSessions} />
      <span className="min-w-0 flex-1 truncate font-mono text-[13px] text-text">{status}</span>
      <Diff sessions={sessions} />
    </button>
  );
}

function SettingsButton() {
  return (
    <button
      type="button"
      onClick={() => void openWindow("settings")}
      aria-label="Kiko settings"
      title="Settings"
      className="-mt-1 -mr-1 rounded-md p-1 text-text-2 transition-colors hover:text-text focus-visible:outline-2 focus-visible:outline-text"
    >
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
        <circle cx="12" cy="12" r="3" />
        <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
      </svg>
    </button>
  );
}

/** With no session yet: whether Claude Code can reach Kiko, and the way to fix it. */
function HooksNotice({ status }: { status: HooksStatus | null }) {
  if (!status) return null;
  const needsInstall = status.status === "not-installed" || !status.upToDate;
  const message =
    status.status === "port-busy" || status.status === "server-error"
      ? status.error
      : needsInstall && status.curlError
        ? "Kiko's hooks need curl. See Settings."
        : status.status === "not-installed"
        ? "Claude Code hooks are not installed."
        : !status.upToDate
          ? "Kiko's hooks need an update."
          : null;
  if (!message) {
    return <p className="font-mono text-xs text-text-2">Start Claude Code in a terminal: Kiko wakes up with it.</p>;
  }
  return (
    <div className="flex items-center justify-between gap-3 rounded-xl border border-border bg-bg px-3 py-2">
      <p className="text-sm text-text">{message}</p>
      <button type="button" className={BUTTON.secondary} onClick={() => void openWindow("settings")}>
        {status.status === "not-installed" ? "Set up" : "Settings"}
      </button>
    </div>
  );
}

/** Expanded panel: Kiko with its bubble, sessions, last steps, then the oldest pending request. */
export function ExpandedContent() {
  const mood = useKikoStore((s) => s.mood);
  const activeSessions = useKikoStore((s) => s.activeSessions);
  const sessions = useKikoStore((s) => s.sessions);
  const activeSessionId = useKikoStore((s) => s.activeSessionId);
  const steps = useKikoStore((s) => (s.activeSessionId ? s.steps[s.activeSessionId] : undefined));
  const pending = useKikoStore((s) => s.pending);
  const hooksStatus = useKikoStore((s) => s.hooksStatus);

  const activeSession = sessions.find((s) => s.id === activeSessionId);
  const head = pending[0];
  const headSession = sessions.find((s) => s.id === head?.request.sessionId);
  const waiting = pending.length - 1;

  return (
    // Never taller than the screen: the content scrolls instead.
    <div className="flex max-h-[680px] flex-col gap-3 overflow-y-auto p-4">
      <div className="flex gap-6">
        <Kiko mood={mood} size={120} activeSessions={activeSessions} />
        <div className="flex min-w-0 flex-1 flex-col">
          {/* Leaves the top band free for Kiko's bubble */}
          <div className="flex h-11 items-start justify-end gap-2 pt-0.5">
            <p className="font-mono text-xs text-text-2">{agentsRunning(sessions.length)}</p>
            <SettingsButton />
          </div>
          <SessionList sessions={sessions} activeSessionId={activeSessionId} />
        </div>
      </div>

      {sessions.length === 0 && <HooksNotice status={hooksStatus} />}

      {activeSession && steps && steps.length > 0 && <StepList project={activeSession.project} steps={steps} />}

      {head?.kind === "permission" && (
        <PermissionCard
          key={head.request.id}
          request={head.request}
          agent={headSession?.agent ?? "claude-code"}
          project={headSession?.project}
        />
      )}
      {head?.kind === "question" && (
        <QuestionCard key={head.request.id} request={head.request} project={headSession?.project} />
      )}
      {waiting > 0 && (
        <p className="font-mono text-[11px] text-text-2">
          {waiting} more {waiting === 1 ? "request" : "requests"} waiting
        </p>
      )}
    </div>
  );
}

function SessionList({ sessions, activeSessionId }: { sessions: Session[]; activeSessionId: string | null }) {
  const selectSession = useKikoStore((s) => s.selectSession);
  const now = useNow(1000);

  return (
    <ul className="flex flex-col gap-1" aria-label="Sessions">
      {sessions.map((session) => {
        const active = session.id === activeSessionId;
        return (
          <li key={session.id}>
            <button
              type="button"
              aria-pressed={active}
              onClick={() => selectSession(session.id)}
              className={`w-full rounded-lg px-2.5 py-1.5 text-left transition-colors ${
                active ? "bg-bg" : "hover:bg-bg/60"
              } ${FOCUS_INSET}`}
            >
              <span className="flex items-baseline justify-between gap-2">
                <span className="truncate text-sm font-semibold text-text">{session.project}</span>
                <span className="shrink-0 font-mono text-[11px] text-text-2">{formatElapsed(now - session.startedAt)}</span>
              </span>
              <span className="block truncate font-mono text-xs text-text-2">{session.currentStep}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

const STEP_GLYPHS: Record<StepStatus, { glyph: string; className: string }> = {
  running: { glyph: "●", className: "text-working animate-pulse motion-reduce:animate-none" },
  done: { glyph: "✓", className: "text-done" },
  error: { glyph: "✕", className: "text-error" },
};

function StepList({ project, steps }: { project: string; steps: Step[] }) {
  const newestFirst = useMemo(() => [...steps].reverse(), [steps]);
  return (
    <section aria-label={`Last steps of ${project}`}>
      <h3 className="mb-1.5 font-mono text-[11px] tracking-wide text-text-2 uppercase">Last steps · {project}</h3>
      <ol className="flex flex-col gap-1">
        {newestFirst.map((step) => {
          const { glyph, className } = STEP_GLYPHS[step.status];
          return (
            <li key={step.id} className="flex items-center gap-2 font-mono text-xs">
              <span role="img" aria-label={step.status} className={`w-3 shrink-0 text-center ${className}`}>
                {glyph}
              </span>
              <span className="w-[4.5rem] shrink-0 truncate text-text">{step.tool}</span>
              <span className="min-w-0 truncate text-text-2">{step.target}</span>
            </li>
          );
        })}
      </ol>
    </section>
  );
}
