import { beforeEach, describe, expect, it } from "vitest";
import { initialData, type KikoData } from "../store/state";
import { setChatSender, useKikoStore } from "../store/useKikoStore";
import type { AgentEvent, ChatMessage, SessionRef } from "../types/events";
import { applyEvent, type KikoEvent } from "./applyEvent";
import { CHANNEL_CLOSED } from "./chat";

// Sessions as the hooks reference's examples name them; chat events as Kiko's server sends them.
const session: SessionRef = { id: "abc123", project: "my-project", cwd: "/home/user/my-project" };
const other: SessionRef = { id: "def456", project: "tumble-run", cwd: "/home/user/tumble-run" };

const started = (s: SessionRef, at = 1_000): AgentEvent => ({ type: "session-started", at, session: s, source: "startup" });
const linked = (sessionId: string): AgentEvent => ({ type: "chat-linked", at: 1_500, sessionId });
const mine = (id: string, text: string, at: number): ChatMessage => ({ id, text, at });
const sending = (sessionId: string, message: ChatMessage): KikoEvent => ({ type: "chat-sending", sessionId, message });
const delivered = (sessionId: string, ...messageIds: string[]): AgentEvent => ({
  type: "chat-delivered",
  at: 2_100,
  sessionId,
  messageIds,
});
const reply = (sessionId: string, id: string, text: string, at = 3_000): AgentEvent => ({
  type: "chat-reply",
  at,
  sessionId,
  message: { id, text, at },
});

const run = (...events: KikoEvent[]): KikoData => events.reduce(applyEvent, initialData);

describe("chat state", () => {
  it("keeps each session's own history: my messages and Claude's replies, with their time", () => {
    const state = run(
      started(session),
      started(other),
      linked("abc123"),
      sending("abc123", mine("m1", "What does `npm test` say?", 2_000)),
      delivered("abc123", "m1"),
      reply("abc123", "reply-1", "All 12 tests pass."),
    );
    expect(state.chatLinks).toEqual({ abc123: true });
    expect(state.chats.abc123).toEqual([
      { id: "m1", from: "you", text: "What does `npm test` say?", at: 2_000, delivery: "delivered" },
      { id: "reply-1", from: "claude", text: "All 12 tests pass.", at: 3_000 },
    ]);
    expect(state.chats.def456).toBeUndefined();
  });

  it("marks a message failed when Kiko can't hand it over, or when the channel closes first", () => {
    let state = run(started(session), linked("abc123"), sending("abc123", mine("m1", "one", 2_000)));
    expect(state.chats.abc123[0].delivery).toBe("sending");
    state = applyEvent(state, { type: "chat-failed", sessionId: "abc123", messageId: "m1", error: "No channel" });
    expect(state.chats.abc123[0]).toMatchObject({ delivery: "failed", error: "No channel" });

    state = run(
      started(session),
      linked("abc123"),
      sending("abc123", mine("m1", "one", 2_000)),
      delivered("abc123", "m1"),
      sending("abc123", mine("m2", "two", 2_200)),
      { type: "chat-unlinked", at: 2_300, sessionId: "abc123" },
    );
    expect(state.chatLinks).toEqual({});
    expect(state.chats.abc123.map((e) => [e.id, e.delivery, e.error])).toEqual([
      ["m1", "delivered", undefined],
      ["m2", "failed", CHANNEL_CLOSED],
    ]);
    // A late delivery report doesn't revive a failed message's spinner; a late failure doesn't undo a delivery.
    state = applyEvent(state, { type: "chat-failed", sessionId: "abc123", messageId: "m1", error: "late" });
    expect(state.chats.abc123[0].delivery).toBe("delivered");
  });

  it("drops the history when its session ends, so the session after /clear starts empty", () => {
    const cleared = run(
      started(session),
      linked("abc123"),
      sending("abc123", mine("m1", "hello", 2_000)),
      reply("abc123", "reply-1", "Hi!"),
      { type: "chat-opened", sessionId: "abc123" },
      { type: "session-ended", at: 4_000, session, reason: "clear" },
      { type: "chat-unlinked", at: 4_000, sessionId: "abc123" },
    );
    expect(cleared.chats).toEqual({});
    expect(cleared.chatLinks).toEqual({});
    expect(cleared.unread).toEqual({});
    expect(cleared.chatSessionId).toBeNull();

    // /clear: same Claude Code process, new session id; its channel follows it.
    const next: SessionRef = { ...session, id: "ghi789" };
    const state = [
      { type: "session-started", at: 4_100, session: next, source: "clear" } as AgentEvent,
      linked("ghi789"),
    ].reduce(applyEvent, cleared);
    expect(state.chatLinks).toEqual({ ghi789: true });
    expect(state.chats.ghi789).toBeUndefined();
  });

  it("counts replies as unread until their chat is on screen", () => {
    let state = run(started(session), linked("abc123"), reply("abc123", "reply-1", "Done."));
    expect(state.unread).toEqual({ abc123: 1 });

    state = applyEvent(state, { type: "chat-opened", sessionId: "abc123" });
    expect(state).toMatchObject({ chatSessionId: "abc123", islandState: "expanded", unread: {} });
    state = applyEvent(state, reply("abc123", "reply-2", "Also done."));
    expect(state.unread).toEqual({});

    // A permission request takes the island over: a reply then is not seen.
    state = applyEvent(state, {
      type: "permission-requested",
      at: 5_000,
      session,
      request: {
        id: "r1",
        sessionId: "abc123",
        tool: "Bash",
        kind: "command",
        subject: "rm -rf node_modules",
        canAlwaysAllow: true,
      },
    });
    state = applyEvent(state, reply("abc123", "reply-3", "Waiting for you."));
    expect(state.unread).toEqual({ abc123: 1 });

    state = applyEvent(state, { type: "chat-closed" });
    expect(state.chatSessionId).toBeNull();
  });

  it("never shows a reply twice", () => {
    const state = run(started(session), reply("abc123", "reply-1", "Hi!"), reply("abc123", "reply-1", "Hi!"));
    expect(state.chats.abc123).toHaveLength(1);
  });
});

describe("sendChat", () => {
  beforeEach(() => {
    useKikoStore.setState(initialData);
    setChatSender(null);
  });

  it("shows the message at once, then hands it to the session's channel", async () => {
    const sent: [string, ChatMessage][] = [];
    setChatSender(async (sessionId, message) => {
      sent.push([sessionId, message]);
    });
    const store = useKikoStore.getState();
    store.dispatch(started(session));
    store.dispatch(linked("abc123"));
    useKikoStore.getState().sendChat("abc123", "Run the tests, please");

    const [entry] = useKikoStore.getState().chats.abc123;
    expect(entry).toMatchObject({ from: "you", text: "Run the tests, please", delivery: "sending" });
    await Promise.resolve();
    expect(sent).toEqual([["abc123", { id: entry.id, text: "Run the tests, please", at: entry.at }]]);

    useKikoStore.getState().dispatch(delivered("abc123", entry.id));
    expect(useKikoStore.getState().chats.abc123[0].delivery).toBe("delivered");
  });

  it("marks the message failed when the session has no channel", async () => {
    setChatSender(() => Promise.reject(new Error("This session has no chat channel.")));
    useKikoStore.getState().dispatch(started(session));
    useKikoStore.getState().sendChat("abc123", "hello?");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(useKikoStore.getState().chats.abc123[0]).toMatchObject({
      delivery: "failed",
      error: "This session has no chat channel.",
    });
  });

  it("gives every message its own id", () => {
    setChatSender(async () => {});
    useKikoStore.getState().sendChat("abc123", "one");
    useKikoStore.getState().sendChat("abc123", "two");
    const [first, second] = useKikoStore.getState().chats.abc123;
    expect(first.id).not.toBe(second.id);
  });
});
