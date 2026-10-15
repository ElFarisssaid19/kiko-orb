# Kiko

Kiko is my BTS DWFS PFE (final-year project). It is a desktop companion that shows AI coding agent sessions (Claude Code first) in a floating island, lets me approve permissions, and lets me chat with a running session.

## Stack

- Tauri 2 (Rust) + React + TypeScript + Vite
- Tailwind CSS v4 (`@tailwindcss/vite`), Zustand
- Fonts: Bricolage Grotesque (UI) and IBM Plex Mono (code), bundled locally via Fontsource. No CDN.
- Linux first (Ubuntu / GNOME). Windows and macOS later.

## Rules

- All OS-specific code lives in `src-tauri/src/platform/` (`linux.rs`, `windows.rs`, `macos.rs`, dispatched by `mod.rs`). No `cfg(target_os)` anywhere else.
- Colors and fonts come from `src/styles/tokens.css` only. Never hardcode a hex value or font name. In components, use the Tailwind utilities mapped in `src/styles/global.css` (`bg-island`, `text-text-2`, `border-border`, `text-working`, `font-ui`, `font-mono`…) or `var(--token)`.
- The UI reads events only through the `EventSource` interface (`src/events/EventSource.ts`). Components never talk to a concrete source directly.
- Never touch `~/.claude/settings.json` unless I explicitly ask.
- Never run `git commit`, `git push`, `git reset`, `git checkout`, `git stash`, or anything else that changes git history. Read-only git (`status`, `diff`, `log`) is fine. I make all commits myself.
- If a system dependency is missing, give me the exact command instead of running `sudo`.
- At the end of each task, list the changed files, explain how to test, and suggest a commit message.

## Layout

```
src/components/   React components (Kiko character, KikoBubble, Island + IslandContent, PermissionCard, QuestionCard, ChatView + markdown, SettingsPanel + ChatSettings)
src/hooks/        React hooks (useKikoAnimation: Kiko's rAF loop; useIslandWindow: native window sizing; useNow; usePrefersReducedMotion)
src/events/       EventSource interface, ClaudeCodeSource (the only source), applyEvent (pure reducer) + chat (its chat part), connect
src/store/        Zustand store (useKikoStore) and its state model (state.ts)
src/types/        Shared TypeScript types (events: mirrors src-tauri/src/hooks/events.rs; kiko)
src/native.ts     Typed wrappers for Kiko's Tauri commands other than events (hooks installer, windows)
src/playground/   Dev-only debug page: live hook log + store state (opened from Settings in dev builds)
src/styles/       tokens.css (design tokens) + global.css (Tailwind entry, fonts, token → utility mapping, animate-* keyframes)
src-tauri/src/hooks/      Claude Code integration: local server (127.0.0.1: hooks + channel endpoints), payload parsing, settings installer
src-tauri/src/chat/       Chat: channel registry (links each channel to its session by process) and the chat installer
src-tauri/src/platform/   OS-specific Rust code
src-tauri/kiko-channel/   kiko-channel, the stdio MCP channel server Claude Code starts in chat sessions (own crate, no Tauri/GTK; its only OS-specific code is src/platform.rs)
docs/             PFE documentation
```

## Commands

- `npm run tauri dev`: run the app (builds kiko-channel, then Vite on port 1420 + Rust)
- `npm run dev`: frontend only, in the browser (no hook server, so no live data)
- `npm test`: Vitest (applyEvent reducer, chat state, reply Markdown) · `cd src-tauri && cargo test --workspace`: Rust tests (parsing, server, installers, chat registry, kiko-channel)
- `npm run build`: type-check and build the frontend
- `cd src-tauri && cargo clippy`: lint the Rust side
- Rust lives in `~/.cargo/bin`. If `cargo` is not found in a non-login shell, run `source ~/.cargo/env` first.

## Claude Code hooks

- Based on the official hooks reference (https://code.claude.com/docs/en/hooks); check it before changing payloads or responses. Test payloads must follow its shapes.
- Every Kiko hook is a shell-form `command` hook: `curl -sf --max-time <T> -H @"$HOME/.config/kiko/auth-header" … -H "X-Kiko-Claude-Pid: $PPID" --data-binary @- http://127.0.0.1:<port>/kiko/hooks/<event> || true`. `$PPID` is the Claude Code process (the hook's `sh -c` is its direct child); chat uses it. Kiko's answer comes back on stdout (JSON = decision, empty = no decision); `-f` + `|| true` make it exit 0 silently when Kiko is closed or refuses the request, so Claude Code never shows an error or blocks.
- Timeouts: curl 115 s on PermissionRequest and on the AskUserQuestion PreToolUse hook (Kiko lets go at 110 s), 2 s on every other event; each hook's `timeout` (seconds) is above its curl limit (120 s / 3 s).
- Kiko's server listens on 127.0.0.1 only (port in `~/.config/kiko/config.json`, default 47821) and rejects requests without the `X-Kiko-Token` header. The token lives in config.json and in `~/.config/kiko/auth-header` (both 0600, rewritten when the token changes); it never appears in settings.json.
- SessionStart and UserPromptSubmit add a hook's plain-text stdout to Claude's context: Kiko answers them with an empty body.
- PermissionRequest and AskUserQuestion (via PreToolUse) are held until the user answers, chooses "Reply in terminal", or 110 s pass; an empty answer lets Claude Code prompt in the terminal as usual.
- Kiko's entries in settings.json are recognized by their endpoint (`http://127.0.0.1:<port>/kiko/hooks/…`), in a curl command or as an old http hook, so a reinstall migrates the hooks of earlier versions (http hooks, then commands without the pid header: "Needs an update"). The installer checks curl (≥ 7.55, for `-H @file`), always backs up the file, shows a preview, refuses to write if the file changed since the preview, and records what it created in `~/.config/kiko/hooks-install.json` so uninstall restores the file exactly.
- Never run the installer on the real `~/.claude/settings.json` while developing: start Kiko with `CLAUDE_CONFIG_DIR=<temp dir>` to point it at a temp settings file.

## Chat (Claude Code channels)

- Based on the channels reference (https://code.claude.com/docs/en/channels-reference); check it before changing the channel. Research preview: needs a claude.ai login, Claude Code ≥ 2.1.80.
- Only chat sessions run the channel: `claude --mcp-config ~/.config/kiko/mcp.json --dangerously-load-development-channels server:kiko`. Kiko never runs `claude mcp add` and never touches `~/.claude.json`. Claude Code matches `server:kiko` by name, so a server from `--mcp-config` registers (its startup notice may still say "no MCP server configured with that name": that check only reads the settings scopes).
- Settings → Chat installs (with a preview of every step): copies `kiko-channel` from next to Kiko's executable to `~/.local/share/kiko/bin/`, writes `~/.config/kiko/mcp.json`, records both in `~/.config/kiko/chat-install.json`; uninstall removes exactly that. It never edits shell files: it only suggests the `kiko-claude` alias.
- Linking: the hooks send `$PPID`, kiko-channel sends its parent pid when it registers; both are the same `claude` process (checked in a real process tree). Kiko links the channel to whatever session that process runs now, so after `/clear` or `/resume` the new SessionStart re-links it. `CLAUDE_CODE_SESSION_ID` (frozen when the channel started) is only a fallback until a hook from that process arrives.
- Server endpoints `/kiko/channel/{register,poll,reply,unregister}`, same token as the hooks. A poll is held up to 25 s, then answered empty; a channel that stops polling for 10 s is dropped. UI events: `chat-linked`, `chat-unlinked`, `chat-delivered`, `chat-reply`. Chat history lives in the island's memory and goes with its session.
- Never read the user's Claude login or credential files. End-to-end chat is tested by hand (README / task notes).

## Linux notes

- The main window is transparent, undecorated, always on top, skips the taskbar and does not take focus on start. It starts hidden. `platform/linux.rs` places it flush against the top of the primary monitor's work area, centered, then shows it.
- The window is only as big as the island plus a small transparent margin. The frontend resizes it with the `set_island_size(width, height)` command (logical px; `platform/linux.rs` keeps it centered and re-asserts always-on-top and skip-taskbar). Rule: grow the window before the island grows, shrink it after the island has finished shrinking (`useIslandWindow`), so nothing is ever clipped.
- The size is locked with min = max size constraints, not `resizable: false`: GTK snaps non-resizable windows to the webview's natural size (420×200). `set_island_size` moves both bounds in one `set_size_constraints` call.
- Use `inner_size()`, not `outer_size()`, before the window is shown: tao reports an outer size of 0 until the first configure event.
- On a Wayland session, `platform/linux.rs` sets `GDK_BACKEND=x11` (XWayland), because GTK3 on native Wayland cannot do always-on-top, absolute positioning or skip-taskbar. Launching with `GDK_BACKEND` already set overrides this.
- The VS Code snap leaks `GTK_PATH`, `GIO_MODULE_DIR`, etc. into its terminal, which crashes WebKitGTK ("symbol lookup error … /snap/core20"). `platform/linux.rs` drops those variables when they point into `/snap/`.
- Design reference for the character: docs/design/kiko-orb.png. The app draws Kiko as SVG in code; never import this PNG in the app.
