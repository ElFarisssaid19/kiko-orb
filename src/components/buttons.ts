// Button styles shared by the island cards. All real <button> elements with a visible keyboard focus ring.

const FOCUS = "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-text";
const BASE = `rounded-lg px-3 py-1.5 text-sm transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${FOCUS}`;

export const BUTTON = {
  /** Filled amber: the action Kiko is waiting for. */
  primary: `${BASE} bg-needs-you font-semibold text-island hover:brightness-110`,
  secondary: `${BASE} border border-border bg-island text-text hover:bg-border`,
  ghost: `${BASE} text-text-2 hover:bg-border/60 hover:text-text`,
  /** Compact secondary button with an icon, e.g. "Chat" next to a section title. */
  small: `flex shrink-0 items-center gap-1.5 rounded-lg border border-border bg-island px-2.5 py-1 text-xs text-text transition-colors hover:bg-border ${FOCUS}`,
  /** Looks like a link. */
  text: `rounded px-1 py-1.5 text-sm text-text-2 underline-offset-4 hover:text-text hover:underline ${FOCUS}`,
  /** Selectable option in a list; add the selected/unselected colors. */
  option: `flex w-full items-center gap-2.5 rounded-lg border px-3 py-1.5 text-left text-sm transition-colors ${FOCUS}`,
};
