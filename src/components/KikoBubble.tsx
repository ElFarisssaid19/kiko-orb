import { useEffect, useState, type Ref } from "react";
import type { Mood } from "../types/kiko";

/** Moods that show a bubble, with their border color. Every other mood has none. */
const BUBBLE_COLORS = {
  working: "var(--working)",
  needsYou: "var(--needs-you)",
} satisfies Partial<Record<Mood, string>>;

type BubbleMood = keyof typeof BUBBLE_COLORS;

const hasBubble = (mood: Mood): mood is BubbleMood => mood in BUBBLE_COLORS;

/** --animate-bubble-out in global.css is 150ms; this is only a fallback if animationend never fires. */
const EXIT_FALLBACK_MS = 300;

const ENTER: Record<BubbleMood, string> = {
  working: "animate-bubble-in motion-reduce:animate-bubble-fade-in",
  needsYou: "animate-bubble-pop motion-reduce:animate-bubble-fade-in",
};
const EXIT = "animate-bubble-out motion-reduce:animate-bubble-fade-out";

export interface KikoBubbleProps {
  mood: Mood;
  /** Message shown while needsYou. */
  text: string;
  /** Positioning wrapper. Kiko's animation loop moves it with the orb and wiggles it while needsYou. */
  ref?: Ref<HTMLDivElement>;
}

/** Speech bubble at Kiko's top right: typing dots while working, a message while needsYou. */
export function KikoBubble({ mood, text, ref }: KikoBubbleProps) {
  const target = hasBubble(mood) ? mood : null;
  const [shown, setShown] = useState<BubbleMood | null>(target);
  if (shown === null && target !== null) setShown(target);
  const leaving = shown !== null && shown !== target;

  // A bubble that no longer matches the mood plays its exit animation first (see onAnimationEnd),
  // then makes room for the next one.
  useEffect(() => {
    if (!leaving) return;
    const timer = setTimeout(() => setShown(target), EXIT_FALLBACK_MS);
    return () => clearTimeout(timer);
  }, [leaving, target]);

  return (
    // Stays mounted while Kiko is, so screen readers announce messages that appear in it.
    <div
      ref={ref}
      role="status"
      aria-live="polite"
      className="absolute"
      style={{ left: "70%", bottom: "74%", transformOrigin: "0 100%" }}
    >
      {shown && (
        <div
          key={shown}
          className={`relative w-max max-w-[180px] origin-bottom-left rounded-[14px] border border-border bg-island px-3 py-2 font-ui text-[14px] leading-5 text-text ${leaving ? EXIT : ENTER[shown]}`}
          style={{ borderColor: BUBBLE_COLORS[shown] }}
          onAnimationEnd={(event) => {
            if (leaving && event.target === event.currentTarget) setShown(target);
          }}
        >
          {shown === "working" ? <TypingDots /> : text}
          <Tail color={BUBBLE_COLORS[shown]} />
        </div>
      )}
    </div>
  );
}

/** Messaging-app "typing" indicator: three dots bouncing one after the other. */
function TypingDots() {
  return (
    <span className="flex h-5 items-center gap-1.5" aria-hidden="true">
      {[0, 150, 300].map((delay) => (
        <span
          key={delay}
          className="size-1.5 animate-typing-dot rounded-full bg-working motion-reduce:animate-none"
          style={{ animationDelay: `${delay}ms` }}
        />
      ))}
    </span>
  );
}

/** Tail at the bottom-left corner, pointing down-left at the orb. Its fill hides the bubble's border where they join. */
function Tail({ color }: { color: string }) {
  return (
    <svg
      className="absolute top-full left-2.5 overflow-visible"
      width="14"
      height="10"
      viewBox="0 0 14 10"
      aria-hidden="true"
    >
      <path d="M4 0 L0 10 L14 0 Z" style={{ fill: "var(--island)" }} />
      <path d="M4 0 L0 10 L14 0" fill="none" strokeLinejoin="round" style={{ stroke: color }} />
    </svg>
  );
}
