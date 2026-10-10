import { invoke, isTauri } from "@tauri-apps/api/core";
import { useMemo, useRef } from "react";

/** Transparent room kept around the island inside the window (px), for the spring overshoot. */
export const WINDOW_PAD_X = 12;
export const WINDOW_PAD_BOTTOM = 16;

/** Fallback: refit this long after the island's size last changed, in case no animation reports its end. */
const SETTLE_MS = 700;
/** Longest wait for the webview to report the new window size. */
const RESIZE_TIMEOUT_MS = 300;

export interface Size {
  width: number;
  height: number;
}

const same = (a: Size | null, b: Size) => a !== null && a.width === b.width && a.height === b.height;

/** Window size that fits an island element: its box plus the transparent padding. */
function fitting(island: HTMLElement): Size {
  return {
    width: island.offsetWidth + 2 * WINDOW_PAD_X,
    height: island.offsetTop + island.offsetHeight + WINDOW_PAD_BOTTOM,
  };
}

/** Resizes the native window, then waits until the webview has its new size so nothing gets clipped. */
async function resizeNativeWindow({ width, height }: Size) {
  if (!isTauri()) return; // playground in a browser tab: nothing to resize
  await invoke("set_island_size", { width, height });
  const deadline = performance.now() + RESIZE_TIMEOUT_MS;
  while ((window.innerWidth !== width || window.innerHeight !== height) && performance.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 16));
  }
}

/**
 * Keeps the native window just big enough for the island:
 * - `grow(size)` before the island grows, so its animation is never clipped;
 * - `fit()` once it has finished shrinking, to the island's exact size.
 * Attach `ref` to the island element: size changes inside it grow the window right away and refit when settled.
 */
export function useIslandWindow() {
  const island = useRef<HTMLElement | null>(null);
  const current = useRef<Size | null>(null);
  const queue = useRef<Promise<void>>(Promise.resolve());
  const settleTimer = useRef<number | undefined>(undefined);

  return useMemo(() => {
    // Resizes run one after another; `next` sees the size left by the previous one.
    const resize = (next: (current: Size | null) => Size) => {
      queue.current = queue.current
        .then(async () => {
          const size = next(current.current);
          if (same(current.current, size)) return;
          current.current = size;
          await resizeNativeWindow(size);
        })
        .catch((error) => console.error("set_island_size failed:", error));
      return queue.current;
    };

    const grow = (size: Size) =>
      resize((cur) => (cur ? { width: Math.max(cur.width, size.width), height: Math.max(cur.height, size.height) } : size));

    const fit = () => {
      const el = island.current;
      if (el) void resize(() => fitting(el));
    };

    const fitWhenSettled = () => {
      clearTimeout(settleTimer.current);
      settleTimer.current = window.setTimeout(fit, SETTLE_MS);
    };

    const ref = (el: HTMLElement | null) => {
      if (!el) return;
      island.current = el;
      const observer = new ResizeObserver(() => {
        if (island.current !== el) return;
        void grow(fitting(el));
        fitWhenSettled();
      });
      observer.observe(el);
      return () => {
        observer.disconnect();
        if (island.current === el) island.current = null;
      };
    };

    return { ref, grow, fit, fitWhenSettled };
  }, []);
}
