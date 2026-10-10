// Fake data for development only (loaded by main.tsx when import.meta.env.DEV).
import { useKikoStore } from "../store/useKikoStore";
import type { PermissionRequest, QuestionRequest, Session, Step } from "../types/events";

const minutesAgo = (minutes: number) => Date.now() - minutes * 60_000;

export function fixtureSessions(): Session[] {
  return [
    {
      id: "s-kiko",
      project: "kiko-orb",
      agent: "claude-code",
      currentStep: "Editing app.tsx",
      startedAt: minutesAgo(12),
      linesAdded: 48,
      linesRemoved: 12,
    },
    {
      id: "s-tumble",
      project: "tumble-run",
      agent: "claude-code",
      currentStep: "Running npm test",
      startedAt: minutesAgo(3),
      linesAdded: 6,
      linesRemoved: 2,
    },
  ];
}

export const fixtureSteps: Record<string, Step[]> = {
  "s-kiko": [
    { id: "k1", tool: "Read", target: "package.json", status: "done" },
    { id: "k2", tool: "Grep", target: "useKikoStore", status: "done" },
    { id: "k3", tool: "Bash", target: "npm run build", status: "error" },
    { id: "k4", tool: "Read", target: "src/App.tsx", status: "done" },
    { id: "k5", tool: "Edit", target: "src/App.tsx", status: "running" },
  ],
  "s-tumble": [
    { id: "t1", tool: "Read", target: "src/game/physics.ts", status: "done" },
    { id: "t2", tool: "Edit", target: "src/game/physics.ts", status: "done" },
    { id: "t3", tool: "Bash", target: "npm test", status: "running" },
  ],
};

export const fixturePermission: PermissionRequest = {
  sessionId: "s-kiko",
  tool: "Bash",
  command: "npm install motion",
};

export const fixtureQuestion: QuestionRequest = {
  sessionId: "s-tumble",
  question: "Which physics engine should the game use?",
  options: ["Matter.js", "Rapier", "Keep the custom one"],
  multiSelect: false,
};

/** Two running sessions, island compact, nothing pending. */
export function loadFixtures() {
  const store = useKikoStore.getState();
  store.reset();
  store.setSessions(fixtureSessions());
  store.setSteps(fixtureSteps);
  store.setIslandState("compact");
}
