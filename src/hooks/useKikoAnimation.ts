import { createRef, useLayoutEffect, useRef, useState, type RefObject } from "react";
import type { Mood, Point } from "../types/kiko";

// Geometry of the reference drawing (viewBox 0 0 240 300).
const ORB = { x: 120, y: 150 };
const RING = { cx: 120, cy: 160, rx: 104, ry: 26 };
const SHADOW = { x: 120, y: 268 };
export const EYE_CENTERS = [
  { x: 101, y: 142 },
  { x: 139, y: 142 },
] as const;

/** Satellite angle (rad) where the reference draws it, front right of the ring. */
const SATELLITE_REST_ANGLE = 0.6;

const FLOAT_AMPLITUDE = 8; // viewBox units
const FLOAT_PERIOD = 4; // s
const PULSE_AMPLITUDE = 0.035; // scale
const PULSE_PERIOD = 1.5; // s
const BOUNCE_HEIGHT = 14;
const BOUNCE_MS = 500;
const SHAKE_AMPLITUDE = 5;
const SHAKE_MS = 450;
const BLINK_EVERY_MS: [number, number] = [3000, 6000];
const BLINK_SQUASH = 2 / 8; // 8px tall eyes squash to 2px
const BLINK_CLOSE_MS = 40;
const BLINK_HOLD_MS = 120;
const GLANCE_EVERY_MS: [number, number] = [2500, 4500];
const GLANCE_MS = 800;
const GLANCE_SHIFT = 6;
const LOOK_SHIFT = { x: 6, y: 4 };
const LOOK_FALLOFF = 120; // distance (viewBox units) at which eyes reach half of LOOK_SHIFT

// Smoothing time constants (s): a value covers ~95% of the way to its target in 3τ.
const MORPH_TAU = 0.08;
const LOOK_TAU = 0.1;
const MOTION_TAU = 0.6;

/** A quadratic stroke from (ax, ay) to (bx, by) via (qx, qy), relative to the eye center, `w` thick. */
type Stroke = [ax: number, ay: number, qx: number, qy: number, bx: number, by: number, w: number];
/** Each eye is two round-capped strokes. They coincide for every mood except error, where they cross. */
type EyeShape = [Stroke, Stroke];
type MouthShape = [halfWidth: number, endY: number, controlY: number];

const line = (half: number, w: number): Stroke => [-half, 0, 0, 0, half, 0, w];
const both = (stroke: Stroke): EyeShape => [stroke, stroke];

const EYES: Record<Mood, EyeShape> = {
  idle: both(line(7, 8)), // 22×8 pill, as in the reference
  working: both(line(7, 8)),
  sleeping: both(line(9.5, 3)), // thin closed line
  needsYou: both(line(0.01, 16)), // wide round dot
  done: both([-9, 4, 0, -10, 9, 4, 4]), // happy arc ^
  error: [
    [-5, -5, 0, 0, 5, 5, 3.5], // small x
    [-5, 5, 0, 0, 5, -5, 3.5],
  ],
};

const MOUTHS: Record<Mood, MouthShape> = {
  idle: [12, 168, 178], // reference smile
  working: [12, 168, 178],
  sleeping: [8, 170, 173],
  needsYou: [7, 170, 175],
  done: [14, 167, 182],
  error: [9, 172, 164], // slight frown
};

interface MotionTarget {
  float: number;
  orbitSpeed: number; // rad/s
  pulse: number;
  blinks: boolean;
  glances: boolean;
  followsLookAt: boolean;
}

const STILL: MotionTarget = {
  float: 0,
  orbitSpeed: 0,
  pulse: 0,
  blinks: false,
  glances: false,
  followsLookAt: false,
};

function motionFor(mood: Mood, activeSessions: number): MotionTarget {
  const sessions = Math.min(Math.max(activeSessions, 0), 8);
  const orbitSpeed: Record<Mood, number> = {
    sleeping: 0,
    idle: 0.5,
    working: 1 + 0.6 * sessions,
    needsYou: 0.5,
    done: 0.8,
    error: 0.3,
  };
  return {
    float: mood === "sleeping" ? 0 : FLOAT_AMPLITUDE,
    orbitSpeed: orbitSpeed[mood],
    pulse: mood === "needsYou" ? PULSE_AMPLITUDE : 0,
    blinks: mood === "idle",
    glances: mood === "working",
    followsLookAt: mood === "idle" || mood === "needsYou" || mood === "done",
  };
}

export function eyeStrokePath([ax, ay, qx, qy, bx, by]: Stroke): string {
  return `M${f(ax)} ${f(ay)}Q${f(qx)} ${f(qy)} ${f(bx)} ${f(by)}`;
}

export function mouthPath([half, endY, controlY]: MouthShape, dx = 0, dy = 0): string {
  const x = 120 + dx;
  return `M${f(x - half)} ${f(endY + dy)}Q${f(x)} ${f(controlY + dy)} ${f(x + half)} ${f(endY + dy)}`;
}

export function satellitePosition(angle: number) {
  return { x: RING.cx + RING.rx * Math.cos(angle), y: RING.cy + RING.ry * Math.sin(angle) };
}

/** The idle pose, used for the first paint before the loop takes over. */
export const REST_POSE = {
  eyeStroke: EYES.idle[0],
  mouth: MOUTHS.idle,
  satellite: satellitePosition(SATELLITE_REST_ANGLE),
};

/** SVG elements the loop moves. Colors are not animated here: they are CSS transitions in Kiko.tsx. */
export interface KikoRig {
  svg: RefObject<SVGSVGElement | null>;
  shadow: RefObject<SVGEllipseElement | null>;
  body: RefObject<SVGGElement | null>;
  eyes: { group: RefObject<SVGGElement | null>; strokes: RefObject<SVGPathElement | null>[] }[];
  mouth: RefObject<SVGPathElement | null>;
  satelliteBack: RefObject<SVGCircleElement | null>;
  satelliteFront: RefObject<SVGCircleElement | null>;
}

function createRig(): KikoRig {
  const eye = () => ({ group: createRef<SVGGElement>(), strokes: [createRef<SVGPathElement>(), createRef<SVGPathElement>()] });
  return {
    svg: createRef(),
    shadow: createRef(),
    body: createRef(),
    eyes: [eye(), eye()],
    mouth: createRef(),
    satelliteBack: createRef(),
    satelliteFront: createRef(),
  };
}

interface AnimationInput {
  mood: Mood;
  activeSessions: number;
  lookAt: Point | null;
}

/** Everything the loop smooths or schedules between frames. */
interface AnimationState {
  mood: Mood | null; // mood seen by the previous frame; null before the first one
  moodSince: number;
  last: number;
  eyes: number[]; // EyeShape, flattened
  mouth: number[];
  lookX: number;
  lookY: number;
  float: number;
  floatPhase: number;
  pulse: number;
  pulsePhase: number;
  orbitSpeed: number;
  angle: number;
  blinkStart: number;
  nextBlink: number;
  glanceDir: number;
  glanceEnd: number;
  nextGlance: number;
}

function initialState(): AnimationState {
  return {
    mood: null,
    moodSince: 0,
    last: 0,
    eyes: EYES.idle.flat(),
    mouth: [...MOUTHS.idle],
    lookX: 0,
    lookY: 0,
    float: 0,
    floatPhase: 0,
    pulse: 0,
    pulsePhase: 0,
    orbitSpeed: 0,
    angle: SATELLITE_REST_ANGLE,
    blinkStart: -Infinity,
    nextBlink: 0,
    glanceDir: 1,
    glanceEnd: 0,
    nextGlance: 0,
  };
}

/**
 * Drives Kiko's idle life and mood changes with requestAnimationFrame: float, satellite orbit, blinks,
 * glances, pulse, bounce, shake and eye/mouth morphs. Pauses while the window is hidden, stops by itself
 * once nothing moves (sleeping, reduced motion), and holds still under prefers-reduced-motion.
 */
export function useKikoAnimation(input: AnimationInput): KikoRig {
  const [rig] = useState(createRig);
  const latest = useRef(input);
  const wake = useRef<() => void>(() => {});

  // Runs after every render: hand the new props to the loop and restart it if it had settled.
  useLayoutEffect(() => {
    latest.current = input;
    wake.current();
  });

  useLayoutEffect(() => {
    const state = initialState();
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    let frame = 0;

    const tick = (now: number) => {
      frame = 0;
      const moving = step(rig, state, latest.current, reducedMotion.matches, now);
      if (moving && document.visibilityState === "visible") frame = requestAnimationFrame(tick);
    };
    const start = () => {
      if (frame === 0 && document.visibilityState === "visible") frame = requestAnimationFrame(tick);
    };
    const onVisibilityChange = () => {
      if (document.visibilityState === "visible") start();
      else {
        cancelAnimationFrame(frame);
        frame = 0;
      }
    };

    // First frame synchronously, so the initial mood is on screen before the browser paints.
    step(rig, state, latest.current, reducedMotion.matches, performance.now());
    wake.current = start;
    start();
    document.addEventListener("visibilitychange", onVisibilityChange);
    reducedMotion.addEventListener("change", start);

    return () => {
      cancelAnimationFrame(frame);
      wake.current = () => {};
      document.removeEventListener("visibilitychange", onVisibilityChange);
      reducedMotion.removeEventListener("change", start);
    };
  }, [rig]);

  return rig;
}

/** Advances the animation to `now` and writes it to the DOM. Returns false once Kiko is completely still. */
function step(rig: KikoRig, s: AnimationState, input: AnimationInput, reduced: boolean, now: number): boolean {
  const dt = s.last === 0 ? 0 : Math.min((now - s.last) / 1000, 0.05);
  s.last = now;

  const firstFrame = s.mood === null;
  if (input.mood !== s.mood) {
    s.mood = input.mood;
    s.moodSince = now;
    s.nextBlink = now + randomBetween(BLINK_EVERY_MS);
    s.nextGlance = now + randomBetween(GLANCE_EVERY_MS);
    s.glanceEnd = 0;
  }
  const mood = input.mood;
  const target = reduced ? STILL : motionFor(mood, input.activeSessions);
  const snap = firstFrame || reduced;
  const sinceMood = now - s.moodSince;

  // Eye and mouth shapes morph towards the mood's shape.
  const eyeTarget = EYES[mood].flat();
  const mouthTarget = MOUTHS[mood];
  for (let i = 0; i < s.eyes.length; i++) s.eyes[i] = approach(s.eyes[i], eyeTarget[i], dt, snap ? 0 : MORPH_TAU);
  for (let i = 0; i < s.mouth.length; i++) s.mouth[i] = approach(s.mouth[i], mouthTarget[i], dt, snap ? 0 : MORPH_TAU);

  // Continuous motion fades in and out instead of starting or stopping abruptly.
  s.float = approach(s.float, target.float, dt, snap ? 0 : MOTION_TAU);
  s.pulse = approach(s.pulse, target.pulse, dt, snap ? 0 : MOTION_TAU);
  s.orbitSpeed = approach(s.orbitSpeed, target.orbitSpeed, dt, snap ? 0 : MOTION_TAU);
  s.floatPhase += (dt * 2 * Math.PI) / FLOAT_PERIOD;
  s.pulsePhase += (dt * 2 * Math.PI) / PULSE_PERIOD;
  s.angle += s.orbitSpeed * dt;

  // One-shot reactions when a mood starts.
  const bouncing = !reduced && mood === "done" && sinceMood < BOUNCE_MS;
  const shaking = !reduced && mood === "error" && sinceMood < SHAKE_MS;
  const bounceY = bouncing ? -BOUNCE_HEIGHT * Math.sin((Math.PI * sinceMood) / BOUNCE_MS) : 0;
  const shakeX = shaking ? SHAKE_AMPLITUDE * Math.sin((sinceMood / SHAKE_MS) * 6 * Math.PI) * (1 - sinceMood / SHAKE_MS) : 0;

  // Blinks: a started blink always finishes, even if the mood changes mid-blink.
  if (target.blinks && now >= s.nextBlink) {
    s.blinkStart = now;
    s.nextBlink = now + randomBetween(BLINK_EVERY_MS);
  }
  const blinkScale = reduced ? 1 : blinkAt(now - s.blinkStart);

  // Where the eyes look: a quick glance while working, otherwise towards lookAt.
  let lookX = 0;
  let lookY = 0;
  if (target.glances) {
    if (now >= s.nextGlance) {
      s.glanceDir = Math.random() < 0.5 ? -1 : 1;
      s.glanceEnd = now + GLANCE_MS;
      s.nextGlance = now + randomBetween(GLANCE_EVERY_MS);
    }
    if (now < s.glanceEnd) lookX = s.glanceDir * GLANCE_SHIFT;
  } else if (target.followsLookAt && input.lookAt) {
    [lookX, lookY] = eyeShiftTowards(rig, input.lookAt);
  }
  s.lookX = approach(s.lookX, lookX, dt, snap ? 0 : LOOK_TAU);
  s.lookY = approach(s.lookY, lookY, dt, snap ? 0 : LOOK_TAU);

  // Write the frame.
  const y = s.float * Math.sin(s.floatPhase) + bounceY;
  const scale = 1 + (s.pulse * (1 - Math.cos(s.pulsePhase))) / 2;
  rig.body.current?.setAttribute(
    "transform",
    `translate(${f(shakeX)} ${f(y)}) ${scaleAround(ORB, scale)}`,
  );
  // The shadow shrinks a little as Kiko rises.
  rig.shadow.current?.setAttribute("transform", scaleAround(SHADOW, 1 + y * 0.012));

  rig.eyes.forEach((eye, i) => {
    const c = EYE_CENTERS[i];
    eye.group.current?.setAttribute("transform", `translate(${f(c.x + s.lookX)} ${f(c.y + s.lookY)}) scale(1 ${f(blinkScale)})`);
    eye.strokes.forEach((path, j) => {
      const stroke = s.eyes.slice(j * 7, j * 7 + 7) as Stroke;
      path.current?.setAttribute("d", eyeStrokePath(stroke));
      path.current?.setAttribute("stroke-width", f(stroke[6]));
    });
  });
  rig.mouth.current?.setAttribute("d", mouthPath(s.mouth as MouthShape, s.lookX * 0.6, s.lookY * 0.6));

  const sat = satellitePosition(s.angle);
  const behind = Math.sin(s.angle) < 0;
  for (const [circle, visible] of [
    [rig.satelliteBack.current, behind],
    [rig.satelliteFront.current, !behind],
  ] as const) {
    circle?.setAttribute("cx", f(sat.x));
    circle?.setAttribute("cy", f(sat.y));
    circle?.setAttribute("visibility", visible ? "visible" : "hidden");
  }

  const settled =
    target.float === 0 &&
    target.orbitSpeed === 0 &&
    target.pulse === 0 &&
    !target.blinks &&
    !target.glances &&
    !bouncing &&
    !shaking &&
    blinkScale === 1 &&
    Math.abs(s.float) < 0.05 &&
    Math.abs(s.orbitSpeed) < 0.005 &&
    s.pulse < 0.0001 &&
    Math.abs(s.lookX - lookX) < 0.01 &&
    Math.abs(s.lookY - lookY) < 0.01 &&
    s.eyes.every((v, i) => Math.abs(v - eyeTarget[i]) < 0.01) &&
    s.mouth.every((v, i) => Math.abs(v - mouthTarget[i]) < 0.01);
  return !settled;
}

/** Eye offset (viewBox units) towards a viewport point: grows with distance and saturates at LOOK_SHIFT. */
function eyeShiftTowards(rig: KikoRig, point: Point): [number, number] {
  const rect = rig.svg.current?.getBoundingClientRect();
  if (!rect || rect.width === 0) return [0, 0];
  const unit = rect.width / 240;
  const dx = (point.x - (rect.left + ORB.x * unit)) / unit;
  const dy = (point.y - (rect.top + ORB.y * unit)) / unit;
  const distance = Math.hypot(dx, dy);
  if (distance === 0) return [0, 0];
  const reach = distance / (distance + LOOK_FALLOFF);
  return [(dx / distance) * LOOK_SHIFT.x * reach, (dy / distance) * LOOK_SHIFT.y * reach];
}

/** Vertical eye scale `elapsed` ms into a blink: close, hold at 2px, reopen. */
function blinkAt(elapsed: number): number {
  if (elapsed < 0 || elapsed >= 2 * BLINK_CLOSE_MS + BLINK_HOLD_MS) return 1;
  if (elapsed < BLINK_CLOSE_MS) return 1 - (1 - BLINK_SQUASH) * (elapsed / BLINK_CLOSE_MS);
  if (elapsed < BLINK_CLOSE_MS + BLINK_HOLD_MS) return BLINK_SQUASH;
  return BLINK_SQUASH + (1 - BLINK_SQUASH) * ((elapsed - BLINK_CLOSE_MS - BLINK_HOLD_MS) / BLINK_CLOSE_MS);
}

/** Frame-rate independent exponential smoothing; tau = 0 jumps straight to the target. */
function approach(current: number, target: number, dt: number, tau: number): number {
  if (tau === 0) return target;
  return current + (target - current) * (1 - Math.exp(-dt / tau));
}

function scaleAround(c: { x: number; y: number }, scale: number): string {
  return `translate(${c.x} ${c.y}) scale(${f(scale, 4)}) translate(${-c.x} ${-c.y})`;
}

function randomBetween([min, max]: [number, number]): number {
  return min + Math.random() * (max - min);
}

function f(n: number, digits = 2): string {
  return n.toFixed(digits);
}
