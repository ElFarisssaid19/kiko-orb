import { describe, expect, it } from "vitest";
import { initialData, type KikoData } from "../store/state";
import type { AgentEvent, SessionRef } from "../types/events";
import { applyEvent, describeStep, type KikoEvent } from "./applyEvent";

// Events as Kiko's hook server builds them from the payloads in the hooks reference
// (https://code.claude.com/docs/en/hooks): session_id "abc123", cwd "/home/user/my-project", …
const session: SessionRef = { id: "abc123", project: "my-project", cwd: "/home/user/my-project" };
const other: SessionRef = { id: "def456", project: "tumble-run", cwd: "/home/user/tumble-run" };

const started: AgentEvent = { type: "session-started", at: 1_000, session, source: "startup" };
const prompt: AgentEvent = { type: "prompt-submitted", at: 2_000, session };
/** PreToolUse common input example: Bash "npm test". */
const bashStarted: AgentEvent = {
  type: "tool-started",
  at: 3_000,
  session,
  step: { id: "toolu_01ABC123...", tool: "Bash", target: "npm test", status: "running" },
};
/** PostToolUse Write example: one line written to /path/to/file.txt. */
const writeFinished: AgentEvent = {
  type: "tool-finished",
  at: 4_000,
  session,
  step: { id: "toolu_02", tool: "Write", target: "/path/to/file.txt", status: "done" },
  linesAdded: 1,
  linesRemoved: 0,
};
/** PermissionRequest example: Bash "rm -rf node_modules", with an addRules suggestion. */
const permission = (id: string, from: SessionRef, at: number): AgentEvent => ({
  type: "permission-requested",
  at,
  session: from,
  request: {
    id,
    sessionId: from.id,
    tool: "Bash",
    kind: "command",
    subject: "rm -rf node_modules",
    canAlwaysAllow: true,
  },
});
/** AskUserQuestion example: "Which framework?" React / Vue. */
const question: AgentEvent = {
  type: "question-asked",
  at: 5_000,
  session,
  request: {
    id: "r9",
    sessionId: session.id,
    questions: [
      {
        question: "Which framework?",
        header: "Framework",
        options: [
          { label: "React", description: "Component library" },
          { label: "Vue", description: "Progressive framework" },
        ],
        multiSelect: false,
      },
    ],
  },
};

const run = (...events: KikoEvent[]): KikoData => events.reduce(applyEvent, initialData);

describe("applyEvent", () => {
  it("starts empty: sleeping, island hidden", () => {
    expect(initialData.mood).toBe("sleeping");
    expect(initialData.islandState).toBe("hidden");
    expect(initialData.sessions).toEqual([]);
  });

  it("shows a new session in the compact island, idle until a prompt", () => {
    const state = run(started);
    expect(state.sessions).toHaveLength(1);
    expect(state.sessions[0]).toMatchObject({ id: "abc123", project: "my-project", status: "idle" });
    expect(state.mood).toBe("idle");
    expect(state.islandState).toBe("compact");
    expect(state.activeSessionId).toBe("abc123");
  });

  it("works through a turn: prompt, tools, lines changed", () => {
    const state = run(started, prompt, bashStarted, writeFinished);
    expect(state.mood).toBe("working");
    expect(state.activeSessions).toBe(1);
    expect(state.sessions[0]).toMatchObject({ currentStep: "Running npm test", linesAdded: 1, linesRemoved: 0 });
    expect(state.steps.abc123.map((s) => [s.tool, s.status])).toEqual([
      ["Bash", "running"],
      ["Write", "done"],
    ]);
  });

  it("finishes a running step in place instead of adding a second one", () => {
    const finished: AgentEvent = { ...writeFinished, step: { ...bashStarted.step, status: "error" }, linesAdded: 0 };
    const state = run(bashStarted, finished);
    expect(state.steps.abc123).toEqual([{ ...bashStarted.step, status: "error" }]);
  });

  it("keeps only the last 5 steps", () => {
    const steps = Array.from({ length: 7 }, (_, i): AgentEvent => ({
      ...bashStarted,
      step: { ...bashStarted.step, id: `toolu_${i}` },
    }));
    expect(run(...steps).steps.abc123.map((s) => s.id)).toEqual(["toolu_2", "toolu_3", "toolu_4", "toolu_5", "toolu_6"]);
  });

  it("picks up a session that started before Kiko from any of its events", () => {
    const state = run(bashStarted);
    expect(state.sessions[0]).toMatchObject({ id: "abc123", status: "working", startedAt: 3_000 });
    expect(state.islandState).toBe("compact");
  });

  it("opens on a permission, queues them oldest first, and goes back to compact after the last answer", () => {
    let state = run(started, prompt, permission("r1", session, 6_000));
    expect(state.mood).toBe("needsYou");
    expect(state.islandState).toBe("expanded");

    state = run(started, prompt, permission("r1", session, 6_000), permission("r2", other, 7_000));
    expect(state.pending.map((p) => p.request.id)).toEqual(["r1", "r2"]);
    expect(state.activeSessionId).toBe("abc123");

    state = applyEvent(state, { type: "answered", requestId: "r1", response: { kind: "permission", decision: "allow" } });
    expect(state.pending.map((p) => p.request.id)).toEqual(["r2"]);
    expect(state.islandState).toBe("expanded");
    expect(state.activeSessionId).toBe("def456");
    expect(state.lastDecision).toEqual({ requestId: "r1", response: { kind: "permission", decision: "allow" } });

    state = applyEvent(state, { type: "answered", requestId: "r2", response: { kind: "permission", decision: "deny" } });
    expect(state.pending).toEqual([]);
    expect(state.islandState).toBe("compact");
    expect(state.mood).toBe("working");
  });

  it("never lists a request twice when a window catches up", () => {
    const state = run(started, permission("r1", session, 6_000), permission("r1", session, 6_000));
    expect(state.pending).toHaveLength(1);
  });

  it("drops a request the server let go (timeout, reply in terminal)", () => {
    const state = run(started, prompt, permission("r1", session, 6_000), {
      type: "request-resolved",
      at: 116_000,
      requestId: "r1",
      reason: "timeout",
    });
    expect(state.pending).toEqual([]);
    expect(state.mood).toBe("working");
    expect(state.islandState).toBe("compact");
  });

  it("asks questions like permissions", () => {
    const state = run(started, prompt, question);
    expect(state.mood).toBe("needsYou");
    expect(state.pending[0]).toMatchObject({ kind: "question", request: { id: "r9" } });
    const answered = applyEvent(state, {
      type: "answered",
      requestId: "r9",
      response: { kind: "question", answers: { "Which framework?": "React" } },
    });
    expect(answered.pending).toEqual([]);
  });

  it("needs you while Claude Code waits in the terminal", () => {
    const waiting = run(started, prompt, {
      type: "notification",
      at: 8_000,
      session,
      notificationType: "permission_prompt",
      message: "Claude needs your permission",
    });
    expect(waiting.mood).toBe("needsYou");
    expect(applyEvent(waiting, writeFinished).mood).toBe("working");
  });

  it("is done for a moment after a turn, then idle", () => {
    const stopped = run(started, prompt, bashStarted, { type: "turn-stopped", at: 9_000, session });
    expect(stopped.mood).toBe("done");
    expect(stopped.sessions[0].currentStep).toBe("Done");
    const settled = applyEvent(stopped, { type: "done-expired", sessionId: "abc123" });
    expect(settled.mood).toBe("idle");
  });

  it("a turn that ends drops its session's waiting requests, not the others'", () => {
    const state = run(
      started,
      permission("r1", session, 6_000),
      permission("r2", other, 7_000),
      { type: "turn-stopped", at: 9_000, session },
    );
    expect(state.pending.map((p) => p.request.id)).toEqual(["r2"]);
  });

  it("shows an API error until the next prompt", () => {
    // StopFailure example: error "rate_limit".
    const failed = run(started, prompt, { type: "turn-failed", at: 9_000, session, error: "rate_limit" });
    expect(failed.mood).toBe("error");
    expect(failed.sessions[0].currentStep).toBe("API error: rate_limit");
    expect(applyEvent(failed, prompt).mood).toBe("working");
  });

  it("sleeps and hides again when the last session ends", () => {
    // SessionEnd example: reason "other".
    const state = run(started, prompt, permission("r1", session, 6_000), {
      type: "session-ended",
      at: 10_000,
      session,
      reason: "other",
    });
    expect(state.sessions).toEqual([]);
    expect(state.pending).toEqual([]);
    expect(state.steps).toEqual({});
    expect(state.mood).toBe("sleeping");
    expect(state.islandState).toBe("hidden");
  });

  it("describes steps in plain words", () => {
    expect(describeStep({ id: "1", tool: "Edit", target: "/home/user/my-project/src/app.tsx", status: "running" })).toBe(
      "Editing app.tsx",
    );
    expect(describeStep({ id: "2", tool: "WebFetch", target: "https://code.claude.com/docs/en/hooks", status: "running" })).toBe(
      "Reading code.claude.com",
    );
  });
});
