import { useEffect, useState } from "react";
import { Kiko } from "../components/Kiko";
import type { Mood, Point } from "../types/kiko";

const MOODS: { mood: Mood; label: string; dot: string }[] = [
  { mood: "sleeping", label: "Sleeping", dot: "bg-text-2" },
  { mood: "idle", label: "Idle", dot: "bg-working" },
  { mood: "working", label: "Working", dot: "bg-working" },
  { mood: "needsYou", label: "Needs you", dot: "bg-needs-you" },
  { mood: "done", label: "Done", dot: "bg-done" },
  { mood: "error", label: "Error", dot: "bg-error" },
];

/** Dev page for the character, opened with `?playground`. */
export default function Playground() {
  const [mood, setMood] = useState<Mood>("idle");
  const [activeSessions, setActiveSessions] = useState(2);
  const [lookAt, setLookAt] = useState<Point | null>(null);

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
    <div className="flex h-full flex-col items-center justify-center gap-10 overflow-auto bg-bg p-8 text-text">
      <div className="flex items-end gap-16">
        <Kiko
          mood={mood}
          activeSessions={activeSessions}
          lookAt={lookAt}
          onDoneFinished={() => setMood("idle")}
        />
        <div className="flex flex-col items-center gap-3">
          <Kiko mood={mood} size={28} activeSessions={activeSessions} lookAt={lookAt} />
          <span className="font-mono text-xs text-text-2">28px</span>
        </div>
      </div>

      <div className="flex flex-wrap justify-center gap-2">
        {MOODS.map(({ mood: value, label, dot }) => (
          <button
            key={value}
            type="button"
            onClick={() => setMood(value)}
            className={`flex items-center gap-2 rounded-full border px-4 py-2 transition-colors ${
              mood === value
                ? "border-border bg-island text-text"
                : "border-transparent text-text-2 hover:text-text"
            }`}
          >
            <span className={`size-2.5 rounded-full ${dot}`} />
            {label}
          </button>
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
    </div>
  );
}
