/** What Kiko is currently expressing. */
export type Mood = "sleeping" | "idle" | "working" | "needsYou" | "done" | "error";

/** A point in viewport (client) coordinates, in px. */
export interface Point {
  x: number;
  y: number;
}
