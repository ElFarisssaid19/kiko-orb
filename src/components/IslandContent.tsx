import { useMemo } from "react";
import { useNow } from "../hooks/useNow";
import { useKikoStore, type KikoState } from "../store/useKikoStore";
import type { Session, Step, StepStatus } from "../types/events";
import { Kiko } from "./Kiko";
import { PermissionCard } from "./PermissionCard";
import { QuestionCard } from "./QuestionCard";

const FOCUS_INSET = "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-text";

function agentsRunning(count: number): string {
  if (count === 0) return "No agents running";
  return `${count} ${count === 1 ? "agent" : "agents"} running`;
}

/** One-line status of the compact island. */
function statusLine(s: KikoState): string {
  if (s.pendingPermission) return "Waiting for your approval";
  if (s.pendingQuestion) return "Claude Code has a question";
  if (s.sessions.length === 1) return s.sessions[0].currentStep;
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

/** Expanded panel: Kiko with its bubble, sessions, last steps of the active session, pending card. */
export function ExpandedContent() {
  const mood = useKikoStore((s) => s.mood);
  const activeSessions = useKikoStore((s) => s.activeSessions);
  const sessions = useKikoStore((s) => s.sessions);
  const activeSessionId = useKikoStore((s) => s.activeSessionId);
  const steps = useKikoStore((s) => (s.activeSessionId ? s.steps[s.activeSessionId] : undefined));
  const pendingPermission = useKikoStore((s) => s.pendingPermission);
  const pendingQuestion = useKikoStore((s) => s.pendingQuestion);

  const activeSession = sessions.find((s) => s.id === activeSessionId);
  const permissionSession = sessions.find((s) => s.id === pendingPermission?.sessionId);

  return (
    <div className="flex flex-col gap-3 p-4">
      <div className="flex gap-3">
        <Kiko mood={mood} size={120} activeSessions={activeSessions} />
        <div className="flex min-w-0 flex-1 flex-col">
          {/* Leaves the top band free for Kiko's bubble */}
          <p className="h-11 pt-0.5 text-right font-mono text-xs text-text-2">{agentsRunning(sessions.length)}</p>
          <SessionList sessions={sessions} activeSessionId={activeSessionId} />
        </div>
      </div>

      {activeSession && steps && steps.length > 0 && <StepList project={activeSession.project} steps={steps} />}

      {pendingPermission ? (
        <PermissionCard
          request={pendingPermission}
          agent={permissionSession?.agent ?? "claude-code"}
          project={permissionSession?.project}
        />
      ) : pendingQuestion ? (
        <QuestionCard key={pendingQuestion.question} request={pendingQuestion} />
      ) : null}
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
              <span className="w-10 shrink-0 text-text">{step.tool}</span>
              <span className="min-w-0 truncate text-text-2">{step.target}</span>
            </li>
          );
        })}
      </ol>
    </section>
  );
}
