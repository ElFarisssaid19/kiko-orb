import { useEffect, useRef, useState } from "react";
import {
  EYE_CENTERS,
  REST_POSE,
  eyeStrokePath,
  mouthPath,
  useKikoAnimation,
} from "../hooks/useKikoAnimation";
import type { Mood, Point } from "../types/kiko";

/** How long Kiko celebrates "done" before going back to idle. */
const DONE_DURATION_MS = 4000;

const FADE = "fill 300ms ease, stroke 300ms ease, opacity 300ms ease";

interface Palette {
  accent: string; // ring and orb outline
  face: string; // eyes and mouth
  satellite: string;
}

const PALETTES: Record<Mood, Palette> = {
  sleeping: { accent: "var(--working)", face: "var(--working)", satellite: "var(--needs-you)" },
  idle: { accent: "var(--working)", face: "var(--working)", satellite: "var(--needs-you)" },
  working: { accent: "var(--working)", face: "var(--working)", satellite: "var(--needs-you)" },
  needsYou: { accent: "var(--needs-you)", face: "var(--needs-you)", satellite: "var(--needs-you)" },
  done: { accent: "var(--done)", face: "var(--done)", satellite: "var(--done)" },
  error: { accent: "var(--working)", face: "var(--error)", satellite: "var(--needs-you)" },
};

export interface KikoProps {
  mood: Mood;
  /** Width in px. The drawing is 4:5, so the height is size × 1.25. */
  size?: number;
  /** Running agent sessions: the satellite orbits faster with more of them while working. */
  activeSessions?: number;
  /** Viewport point (px) the eyes drift towards, e.g. the mouse. */
  lookAt?: Point | null;
  /** Called when the "done" celebration ends and Kiko is back to idle. */
  onDoneFinished?: () => void;
}

export function Kiko({ mood, size = 240, activeSessions = 0, lookAt = null, onDoneFinished }: KikoProps) {
  // "done" turns back into idle by itself after DONE_DURATION_MS.
  const [shownMood, setShownMood] = useState(mood);
  const [previousMood, setPreviousMood] = useState(mood);
  if (mood !== previousMood) {
    setPreviousMood(mood);
    setShownMood(mood);
  }

  const onDoneFinishedRef = useRef(onDoneFinished);
  useEffect(() => {
    onDoneFinishedRef.current = onDoneFinished;
  });

  useEffect(() => {
    if (mood !== "done") return;
    const timer = setTimeout(() => {
      setShownMood("idle");
      onDoneFinishedRef.current?.();
    }, DONE_DURATION_MS);
    return () => clearTimeout(timer);
  }, [mood]);

  const rig = useKikoAnimation({ mood: shownMood, activeSessions, lookAt });
  const colors = PALETTES[shownMood];

  return (
    <svg
      ref={rig.svg}
      viewBox="0 0 240 300"
      width={size}
      height={size * 1.25}
      role="img"
      aria-label={`Kiko (${shownMood})`}
    >
      <g style={{ opacity: shownMood === "sleeping" ? 0.5 : 1, transition: FADE }}>
        <ellipse ref={rig.shadow} cx="120" cy="268" rx="46" ry="8" opacity="0.35" style={{ fill: "var(--orb-shadow)" }} />

        <g ref={rig.body}>
          {/* Back of the ring and satellite, hidden by the orb */}
          <ellipse
            cx="120"
            cy="160"
            rx="104"
            ry="26"
            fill="none"
            strokeWidth="4"
            opacity="0.45"
            style={{ stroke: colors.accent, transition: FADE }}
          />
          <circle
            ref={rig.satelliteBack}
            cx={REST_POSE.satellite.x}
            cy={REST_POSE.satellite.y}
            r="7"
            visibility="hidden"
            style={{ fill: colors.satellite, transition: FADE }}
          />

          {/* Orb */}
          <circle cx="120" cy="150" r="76" style={{ fill: "var(--orb)" }} />
          <circle
            cx="120"
            cy="150"
            r="76"
            fill="none"
            strokeWidth="3"
            opacity="0.6"
            style={{ stroke: colors.accent, transition: FADE }}
          />
          <ellipse
            cx="94"
            cy="112"
            rx="22"
            ry="11"
            opacity="0.14"
            transform="rotate(-30 94 112)"
            style={{ fill: "var(--orb-highlight)" }}
          />

          {/* Face: each eye is two strokes so every mood's shape can morph into the next */}
          <g fill="none" strokeLinecap="round" style={{ stroke: colors.face, transition: FADE }}>
            {rig.eyes.map((eye, i) => (
              <g key={i} ref={eye.group} transform={`translate(${EYE_CENTERS[i].x} ${EYE_CENTERS[i].y})`}>
                {eye.strokes.map((stroke, j) => (
                  <path
                    key={j}
                    ref={stroke}
                    d={eyeStrokePath(REST_POSE.eyeStroke)}
                    strokeWidth={REST_POSE.eyeStroke[6]}
                  />
                ))}
              </g>
            ))}
            <path ref={rig.mouth} d={mouthPath(REST_POSE.mouth)} strokeWidth="4" />
          </g>

          {/* Front of the ring and satellite, over the orb */}
          <path
            d="M16 160 A104 26 0 0 0 224 160"
            fill="none"
            strokeWidth="4"
            style={{ stroke: colors.accent, transition: FADE }}
          />
          <circle
            ref={rig.satelliteFront}
            cx={REST_POSE.satellite.x}
            cy={REST_POSE.satellite.y}
            r="7"
            style={{ fill: colors.satellite, transition: FADE }}
          />
        </g>
      </g>
    </svg>
  );
}
