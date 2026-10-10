import { useEffect, useState, type ReactNode } from "react";
import { Island } from "../components/Island";
import { Kiko } from "../components/Kiko";
import { fixturePermission, fixtureQuestion, loadFixtures } from "../dev/fixtures";
import { useKikoStore, type IslandState } from "../store/useKikoStore";
import type { Mood, Point } from "../types/kiko";

const MOODS: { mood: Mood; label: string; dot: string }[] = [
  { mood: "sleeping", label: "Sleeping", dot: "bg-text-2" },
  { mood: "idle", label: "Idle", dot: "bg-working" },
  { mood: "working", label: "Working", dot: "bg-working" },
  { mood: "needsYou", label: "Needs you", dot: "bg-needs-you" },
  { mood: "done", label: "Done", dot: "bg-done" },
  { mood: "error", label: "Error", dot: "bg-error" },
];

const ISLAND_STATES: IslandState[] = ["hidden", "compact", "expanded"];

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

/** Dev page for the character and the island, opened with `?playground`. */
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
    <div className="flex h-full flex-wrap items-start justify-center gap-16 overflow-auto bg-bg p-8 text-text">
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

      <IslandSection />
    </div>
  );
}

/** The island as it renders in the Tauri window, driven by the store. */
function IslandSection() {
  const islandState = useKikoStore((s) => s.islandState);
  const lastDecision = useKikoStore((s) => s.lastDecision);
  const store = useKikoStore.getState;

  return (
    <section aria-label="Island" className="flex flex-col items-center gap-4">
      {/* Stand-in for the Tauri window at its largest. Hover the hidden pill to peek. */}
      <div className="relative h-[600px] w-[468px] rounded-xl border border-dashed border-border">
        <Island />
      </div>

      <div className="flex flex-wrap justify-center gap-2">
        {ISLAND_STATES.map((state) => (
          <Chip key={state} active={islandState === state} onClick={() => store().setIslandState(state)}>
            {state}
          </Chip>
        ))}
      </div>
      <div className="flex flex-wrap justify-center gap-2">
        <Chip onClick={() => store().requestPermission(fixturePermission)}>
          <span className="size-2.5 rounded-full bg-needs-you" />
          Permission
        </Chip>
        <Chip onClick={() => store().askQuestion(fixtureQuestion)}>
          <span className="size-2.5 rounded-full bg-needs-you" />
          Question
        </Chip>
        <Chip onClick={loadFixtures}>Load fixtures</Chip>
        <Chip onClick={() => store().reset()}>Clear</Chip>
      </div>
      <p className="max-w-[468px] truncate font-mono text-xs text-text-2">
        lastDecision: {lastDecision ? JSON.stringify(lastDecision.kind === "permission" ? lastDecision.decision : lastDecision.answer) : "none"}
      </p>
    </section>
  );
}
