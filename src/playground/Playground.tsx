import { isTauri } from "@tauri-apps/api/core";
import { useEffect, useState, type ReactNode } from "react";
import { Kiko } from "../components/Kiko";
import { connectedSource } from "../events/connect";
import { useKikoStore } from "../store/useKikoStore";
import type { HookLogEntry } from "../types/events";
import type { Mood, Point } from "../types/kiko";

const MOODS: { mood: Mood; label: string; dot: string }[] = [
  { mood: "sleeping", label: "Sleeping", dot: "bg-text-2" },
  { mood: "idle", label: "Idle", dot: "bg-working" },
  { mood: "working", label: "Working", dot: "bg-working" },
  { mood: "needsYou", label: "Needs you", dot: "bg-needs-you" },
  { mood: "done", label: "Done", dot: "bg-done" },
  { mood: "error", label: "Error", dot: "bg-error" },
];

function Chip({ active, onClick, children }: { active?: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex items-center gap-2 rounded-full border px-4 py-2 transition-colors ${
        active ? "border-border bg-island text-text" : "border-transparent text-text-2 hover:text-text"
      }`}
    >
      {children}
    </button>
  );
}

/** Dev page: the character's moods, and the live hook traffic with the store it produces. */
export default function Playground() {
  const [mood, setMood] = useState<Mood>("idle");
  const [activeSessions, setActiveSessions] = useState(2);
  const [lookAt, setLookAt] = useState<Point | null>(null);
  const [bubbleText, setBubbleText] = useState("Where are you?");
  const [showBubble, setShowBubble] = useState(true);

  useEffect(() => {
    const onMove = (event: PointerEvent) => setLookAt({ x: event.clientX, y: event.clientY });
    const onLeave = () => setLookAt(null);
    window.addEventListener("pointermove", onMove);
    document.documentElement.addEventListener("pointerleave", onLeave);
    return () => {
      window.removeEventListener("pointermove", onMove);
      document.documentElement.removeEventListener("pointerleave", onLeave);
    };
  }, []);

  return (
    <div className="flex h-full flex-wrap items-start justify-center gap-10 overflow-auto bg-bg p-8 text-text">
      <section aria-label="Character" className="flex flex-col items-center gap-10 pt-16">
        {/* The mini Kiko sits on the left: the big one's bubble overflows to the right. */}
        <div className="flex items-end gap-16">
          <div className="flex flex-col items-center gap-3">
            <Kiko mood={mood} size={28} activeSessions={activeSessions} lookAt={lookAt} />
            <span className="font-mono text-xs text-text-2">28px</span>
          </div>
          <Kiko
            mood={mood}
            activeSessions={activeSessions}
            lookAt={lookAt}
            onDoneFinished={() => setMood("idle")}
            showBubble={showBubble}
            bubbleText={bubbleText}
          />
        </div>

        <div className="flex max-w-md flex-wrap justify-center gap-2">
          {MOODS.map(({ mood: value, label, dot }) => (
            <Chip key={value} active={mood === value} onClick={() => setMood(value)}>
              <span className={`size-2.5 rounded-full ${dot}`} />
              {label}
            </Chip>
          ))}
        </div>

        <label className="flex items-center gap-4 font-mono text-sm text-text-2">
          activeSessions
          <input
            type="range"
            min={0}
            max={5}
            step={1}
            value={activeSessions}
            onChange={(event) => setActiveSessions(Number(event.target.value))}
            className="accent-working"
          />
          <span className="w-3 text-text">{activeSessions}</span>
        </label>

        <div className="flex flex-wrap items-center justify-center gap-6 font-mono text-sm text-text-2">
          <label className="flex items-center gap-3">
            bubbleText
            <input
              type="text"
              value={bubbleText}
              onChange={(event) => setBubbleText(event.target.value)}
              className="w-56 rounded-lg border border-border bg-island px-3 py-1.5 font-ui text-text outline-none focus:border-needs-you"
            />
          </label>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={showBubble}
              onChange={(event) => setShowBubble(event.target.checked)}
              className="accent-working"
            />
            showBubble
          </label>
        </div>
      </section>

      <LiveSection />
    </div>
  );
}

const time = (at: number) => new Date(at).toLocaleTimeString([], { hour12: false });

/** The last 50 real hook events and the current store state, for debugging. */
function LiveSection() {
  const [log, setLog] = useState<HookLogEntry[]>([]);
  const state = useKikoStore();
  useEffect(() => connectedSource()?.subscribeLog(setLog), []);

  const data = Object.fromEntries(Object.entries(state).filter(([, value]) => typeof value !== "function"));
  const newestFirst = [...log].reverse();

  return (
    <section aria-label="Live hooks" className="flex w-[600px] max-w-full flex-col gap-4">
      <h2 className="text-base font-semibold">Live hook events</h2>
      {!isTauri() && (
        <p className="text-sm text-text-2">
          This tab has no hook server behind it. Open the playground from Kiko's settings window (dev build).
        </p>
      )}
      <div className="max-h-80 overflow-auto rounded-xl border border-border bg-island">
        <table className="w-full font-mono text-xs">
          <thead className="sticky top-0 bg-island text-left text-text-2">
            <tr>
              <th className="px-3 py-2 font-normal">Time</th>
              <th className="px-3 py-2 font-normal">Event</th>
              <th className="px-3 py-2 font-normal">Session</th>
              <th className="px-3 py-2 font-normal">Summary</th>
            </tr>
          </thead>
          <tbody>
            {newestFirst.map((entry, i) => (
              <tr key={`${entry.at}-${i}`} className="border-t border-border align-top">
                <td className="px-3 py-1.5 whitespace-nowrap text-text-2">{time(entry.at)}</td>
                <td className={`px-3 py-1.5 whitespace-nowrap ${entry.event === "Rejected" ? "text-error" : "text-text"}`}>
                  {entry.event}
                </td>
                <td className="px-3 py-1.5 whitespace-nowrap text-text-2" title={entry.sessionId}>
                  {entry.project || "—"} {entry.sessionId.slice(0, 8)}
                </td>
                <td className="px-3 py-1.5 break-all text-text">{entry.summary}</td>
              </tr>
            ))}
            {log.length === 0 && (
              <tr>
                <td colSpan={4} className="px-3 py-3 text-text-2">
                  No hook event yet. Start Claude Code in a terminal once the hooks are installed.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <h2 className="text-base font-semibold">Store</h2>
      <pre className="max-h-96 overflow-auto rounded-xl border border-border bg-island p-3 font-mono text-xs leading-5 text-text">
        {JSON.stringify(data, null, 2)}
      </pre>
    </section>
  );
}
