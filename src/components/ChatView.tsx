import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { openWindow } from "../native";
import { useKikoStore, type ChatEntry, type ChatSetup, type Session } from "../store/useKikoStore";
import { BUTTON } from "./buttons";
import { CopyButton } from "./CopyButton";
import { Kiko } from "./Kiko";
import { parseMarkdown, type Inline } from "./markdown";

const EMPTY: ChatEntry[] = [];

const timeOf = (at: number) => new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

function Inlines({ inlines }: { inlines: Inline[] }) {
  return inlines.map((inline, i) =>
    inline.kind === "code" ? (
      <code key={i} className="rounded border border-border bg-island px-1 font-mono text-[12px]">
        {inline.text}
      </code>
    ) : inline.kind === "bold" ? (
      <strong key={i} className="font-semibold">
        {inline.text}
      </strong>
    ) : (
      <span key={i}>{inline.text}</span>
    ),
  );
}

/** Claude's reply: paragraphs, and code blocks in mono that scroll sideways instead of wrapping. */
function Reply({ text }: { text: string }) {
  const blocks = useMemo(() => parseMarkdown(text), [text]);
  return (
    <div className="flex min-w-0 flex-col gap-2">
      {blocks.map((block, i) =>
        block.kind === "code" ? (
          <pre
            key={i}
            aria-label={block.lang ? `${block.lang} code` : "Code"}
            className="overflow-x-auto rounded-lg border border-border bg-island px-3 py-2 font-mono text-xs leading-5 text-text"
          >
            <code>{block.code}</code>
          </pre>
        ) : (
          <p key={i} className={`break-words whitespace-pre-wrap ${block.kind === "heading" ? "font-semibold" : ""}`}>
            <Inlines inlines={block.inlines} />
          </p>
        ),
      )}
    </div>
  );
}

const DELIVERY = {
  sending: { label: "Sending…", className: "text-text-2" },
  delivered: { label: "Delivered", className: "text-text-2" },
  failed: { label: "Not delivered", className: "text-error" },
};

function Message({ entry }: { entry: ChatEntry }) {
  const you = entry.from === "you";
  const delivery = entry.delivery && DELIVERY[entry.delivery];
  return (
    <li className={`flex min-w-0 flex-col gap-1 ${you ? "items-end" : "items-start"}`}>
      <p className="font-mono text-[11px] text-text-2">
        {you ? "You" : "Claude"} · <time dateTime={new Date(entry.at).toISOString()}>{timeOf(entry.at)}</time>
        {delivery && <span className={delivery.className}> · {delivery.label}</span>}
      </p>
      <div
        className={`min-w-0 text-sm leading-relaxed text-text select-text ${
          you
            ? "max-w-[85%] rounded-2xl rounded-tr-md border border-border bg-bg px-3 py-2 break-words whitespace-pre-wrap"
            : "w-full"
        }`}
      >
        {you ? entry.text : <Reply text={entry.text} />}
      </div>
      {entry.delivery === "failed" && entry.error && <p className="max-w-[85%] text-right text-xs text-error">{entry.error}</p>}
    </li>
  );
}

/** Why this session can't be written to, and the command that starts one that can. */
function NoChannel({ setup }: { setup: ChatSetup | null }) {
  const installed = setup?.installed ?? false;
  return (
    <div className="flex flex-col gap-2 rounded-xl border border-border bg-bg p-3">
      <p className="text-sm text-text">
        {installed
          ? "This session was started without Kiko's chat. To chat with Claude Code, start it with:"
          : "Chat isn't set up yet. Install it in Settings, then start Claude Code with:"}
      </p>
      {setup && (
        <div className="flex items-start gap-2">
          <code className="min-w-0 flex-1 rounded-lg border border-border bg-island px-2 py-1.5 font-mono text-[11px] leading-4 break-all text-text select-text">
            {setup.launchCommand}
          </code>
          <CopyButton text={setup.launchCommand} />
        </div>
      )}
      {!installed && (
        <button type="button" className={`${BUTTON.secondary} self-start`} onClick={() => void openWindow("settings")}>
          Open Settings
        </button>
      )}
    </div>
  );
}

/**
 * One session's chat in the expanded island: the history, then the input. Ctrl+Enter sends; Esc goes
 * back to the overview (handled by the island).
 */
export function ChatView({ session }: { session: Session }) {
  const entries = useKikoStore((s) => s.chats[session.id] ?? EMPTY);
  const linked = useKikoStore((s) => s.chatLinks[session.id] === true);
  const setup = useKikoStore((s) => s.chatSetup);
  const mood = useKikoStore((s) => s.mood);
  const activeSessions = useKikoStore((s) => s.activeSessions);
  const openChat = useKikoStore((s) => s.openChat);
  const closeChat = useKikoStore((s) => s.closeChat);
  const sendChat = useKikoStore((s) => s.sendChat);
  const [draft, setDraft] = useState("");
  const list = useRef<HTMLOListElement>(null);
  const input = useRef<HTMLTextAreaElement>(null);

  // On screen: its replies count as seen, also when the island opens again on this chat.
  useEffect(() => openChat(session.id), [openChat, session.id]);
  // The newest message stays in view.
  useLayoutEffect(() => {
    list.current?.scrollTo({ top: list.current.scrollHeight });
  }, [entries.length]);
  useEffect(() => {
    if (linked) input.current?.focus();
  }, [linked]);

  const text = draft.trim();
  const send = () => {
    if (!linked || !text) return;
    sendChat(session.id, text);
    setDraft("");
  };
  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      send();
    }
  };

  const last = entries[entries.length - 1];
  const working = last?.from === "you" && last.delivery === "delivered" && session.status === "working";

  return (
    <section aria-label={`Chat with ${session.project}`} className="flex flex-col gap-3 p-4">
      <header className="flex items-center gap-2">
        <button
          type="button"
          onClick={closeChat}
          aria-label="Back to sessions"
          title="Back (Esc)"
          className="-ml-1 rounded-md p-1 text-text-2 transition-colors hover:text-text focus-visible:outline-2 focus-visible:outline-text"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
            <path d="M15 18l-6-6 6-6" />
          </svg>
        </button>
        <Kiko mood={mood} size={28} activeSessions={activeSessions} />
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-sm font-semibold text-text">{session.project}</h2>
          <p className="truncate font-mono text-[11px] text-text-2">{linked ? "Chat connected" : "No chat channel"}</p>
        </div>
        <span aria-hidden="true" className={`size-2 shrink-0 rounded-full ${linked ? "bg-done" : "bg-text-2"}`} />
      </header>

      <ol
        ref={list}
        aria-label="Messages"
        aria-live="polite"
        className="flex max-h-[300px] min-h-[72px] flex-col gap-3 overflow-y-auto overscroll-contain pr-1"
      >
        {entries.length === 0 && (
          <li className="m-auto px-4 text-center font-mono text-xs text-text-2">
            {linked ? "Write to Claude Code: its answer shows up here." : "No messages."}
          </li>
        )}
        {entries.map((entry) => (
          <Message key={`${entry.from}-${entry.id}`} entry={entry} />
        ))}
        {working && (
          <li className="flex items-center gap-2 font-mono text-[11px] text-working">
            <span aria-hidden="true" className="animate-pulse motion-reduce:animate-none">
              ●
            </span>
            <span className="truncate">Claude is working · {session.currentStep}</span>
          </li>
        )}
      </ol>

      {!linked && <NoChannel setup={setup} />}

      <div className="flex flex-col gap-1.5">
        <textarea
          ref={input}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onKeyDown}
          disabled={!linked}
          rows={3}
          aria-label={`Message to Claude Code in ${session.project}`}
          placeholder={linked ? "Message Claude Code…" : "This session can't receive messages"}
          className="w-full resize-none rounded-xl border border-border bg-bg px-3 py-2 text-sm text-text select-text placeholder:text-text-2 focus:outline-none focus-visible:ring-2 focus-visible:ring-working/60 disabled:cursor-not-allowed disabled:opacity-50"
        />
        <div className="flex items-center justify-between gap-2">
          <p className="font-mono text-[11px] text-text-2">Ctrl+Enter to send · Esc to leave</p>
          <button type="button" className={BUTTON.primary} disabled={!linked || !text} onClick={send}>
            Send
          </button>
        </div>
      </div>
    </section>
  );
}
