import { AnimatePresence, motion, type Transition } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { WINDOW_PAD_BOTTOM, WINDOW_PAD_X, useIslandWindow, type Size } from "../hooks/useIslandWindow";
import { usePrefersReducedMotion } from "../hooks/usePrefersReducedMotion";
import { useKikoStore, type IslandState, type KikoState } from "../store/useKikoStore";
import { CompactContent, ExpandedContent } from "./IslandContent";

/** What the island shows: a store state, or "peek", the compact pill shown while hovering the hidden island. */
type View = IslandState | "peek";

/** Island shape per view, in px. `top` is the gap from the top edge of the screen's work area. */
const SHAPES: Record<View, { width: number; height: number | "auto"; top: number; radius: number }> = {
  hidden: { width: 120, height: 8, top: 0, radius: 4 },
  peek: { width: 360, height: 44, top: 8, radius: 22 },
  compact: { width: 360, height: 44, top: 8, radius: 22 },
  expanded: { width: 420, height: "auto", top: 8, radius: 24 },
};

/** Taller than any expanded panel: the window grows to this before the panel opens, then fits it. */
const EXPANDED_MAX_HEIGHT = 600;
const PEEK_LEAVE_DELAY_MS = 600;

const SPRING: Transition = { type: "spring", bounce: 0.15, duration: 0.5 };
const CONTENT_IN: Transition = { duration: 0.2, delay: 0.08 };
const CONTENT_OUT: Transition = { duration: 0.12 };
const CROSSFADE: Transition = { duration: 0.2 };

/** Window size needed before switching to `view`. */
function windowSizeFor(view: View): Size {
  const { width, height, top } = SHAPES[view];
  return {
    width: width + 2 * WINDOW_PAD_X,
    height: height === "auto" ? EXPANDED_MAX_HEIGHT : top + height + WINDOW_PAD_BOTTOM,
  };
}

/** Changes whenever the expanded panel's height may change, so the island re-measures and animates. */
const layoutKeyOf = (s: KikoState) =>
  [
    s.sessions.length,
    s.activeSessionId,
    s.activeSessionId ? (s.steps[s.activeSessionId]?.length ?? 0) : 0,
    s.pendingPermission ? 1 : 0,
    s.pendingQuestion?.question ?? "",
  ].join("|");

/**
 * Hover state with a delay on leave, for the hidden island's peek. It starts over each time the island
 * becomes hidden: the window can shrink away from a still pointer without any leave event.
 */
function usePeek(hidden: boolean) {
  const [hovered, setHovered] = useState(false);
  const [wasHidden, setWasHidden] = useState(hidden);
  if (hidden !== wasHidden) {
    setWasHidden(hidden);
    if (hidden) setHovered(false);
  }
  const leaveTimer = useRef<number | undefined>(undefined);
  useEffect(() => () => clearTimeout(leaveTimer.current), []);
  const enter = useCallback(() => {
    clearTimeout(leaveTimer.current);
    setHovered(true);
  }, []);
  const leave = useCallback(() => {
    clearTimeout(leaveTimer.current);
    leaveTimer.current = window.setTimeout(() => setHovered(false), PEEK_LEAVE_DELAY_MS);
  }, []);
  return { hovered, enter, leave };
}

/**
 * The window's only content: a pill at the top center that morphs between hidden, compact and expanded.
 * The native window grows before the island grows and shrinks after it has shrunk, so nothing is clipped.
 */
export function Island() {
  const islandState = useKikoStore((s) => s.islandState);
  const setIslandState = useKikoStore((s) => s.setIslandState);
  const layoutKey = useKikoStore(layoutKeyOf);
  const reduceMotion = usePrefersReducedMotion();
  const peek = usePeek(islandState === "hidden");
  const islandWindow = useIslandWindow();

  const wanted: View = islandState === "hidden" ? (peek.hovered ? "peek" : "hidden") : islandState;
  const [view, setView] = useState<View>(wanted);

  // Switch views only once the window is big enough for the new one.
  useEffect(() => {
    if (wanted === view) {
      islandWindow.fitWhenSettled(); // e.g. a quick back-and-forth left the window bigger than needed
      return;
    }
    let cancelled = false;
    void islandWindow.grow(windowSizeFor(wanted)).then(() => {
      if (!cancelled) setView(wanted);
    });
    return () => {
      cancelled = true;
    };
  }, [wanted, view, islandWindow]);

  // Escape, or focus leaving the window, folds the expanded island back to compact.
  useEffect(() => {
    if (view !== "expanded") return;
    const collapse = () => setIslandState("compact");
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") collapse();
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", collapse);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", collapse);
    };
  }, [view, setIslandState]);

  const expand = () => setIslandState("expanded");
  const content = (v: View) =>
    v === "hidden" ? null : v === "expanded" ? <ExpandedContent /> : <CompactContent onExpand={expand} />;
  const shapeStyle = (v: View) => ({ width: SHAPES[v].width, height: SHAPES[v].height, marginTop: SHAPES[v].top });

  return (
    // Fills the window. Clicking its transparent part (outside the island) folds the expanded island.
    <div
      className="relative flex h-full w-full items-start justify-center"
      onPointerEnter={peek.enter}
      onPointerMove={peek.hovered ? undefined : peek.enter}
      onPointerLeave={peek.leave}
      onPointerDown={(event) => {
        if (event.target === event.currentTarget && view === "expanded") setIslandState("compact");
      }}
    >
      {reduceMotion ? (
        // Reduced motion: the old and new shapes crossfade in place.
        <AnimatePresence initial={false} onExitComplete={islandWindow.fit}>
          <motion.div
            key={view}
            ref={islandWindow.ref}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={CROSSFADE}
            className="absolute inset-x-0 top-0 mx-auto overflow-hidden border border-border bg-island"
            style={{ ...shapeStyle(view), borderRadius: SHAPES[view].radius }}
          >
            {content(view)}
          </motion.div>
        </AnimatePresence>
      ) : (
        // One shape that springs between views while its content fades across.
        <motion.div
          ref={islandWindow.ref}
          layout
          layoutDependency={`${view}|${layoutKey}`}
          initial={false}
          animate={{ borderRadius: SHAPES[view].radius }}
          transition={SPRING}
          onLayoutAnimationComplete={islandWindow.fit}
          className="relative shrink-0 overflow-hidden border border-border bg-island"
          style={shapeStyle(view)}
        >
          <AnimatePresence mode="popLayout" initial={false}>
            {view !== "hidden" && (
              <motion.div
                key={view === "expanded" ? "expanded" : "compact"}
                layout
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: CONTENT_IN }}
                exit={{ opacity: 0, transition: CONTENT_OUT }}
                className="w-full"
              >
                {content(view)}
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      )}
    </div>
  );
}
