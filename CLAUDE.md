# Kiko

Kiko is my BTS DWFS PFE (final-year project). It is a desktop companion that shows AI coding agent sessions (Claude Code first) in a floating island and lets me approve permissions.

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
src/components/   React components (Kiko character, Island, PermissionCard, QuestionCard)
src/events/       EventSource interface + implementations (MockEventSource)
src/store/        Zustand store (useKikoStore)
src/types/        Shared TypeScript types (events)
src/styles/       tokens.css (design tokens) + global.css (Tailwind entry, fonts, token → utility mapping)
src-tauri/src/platform/   OS-specific Rust code
docs/             PFE documentation
```

## Commands

- `npm run tauri dev`: run the app (Vite on port 1420 + Rust)
- `npm run build`: type-check and build the frontend
- `cd src-tauri && cargo clippy`: lint the Rust side
- Rust lives in `~/.cargo/bin`. If `cargo` is not found in a non-login shell, run `source ~/.cargo/env` first.

## Linux notes

- The main window is transparent, undecorated, always on top, skips the taskbar, does not take focus on start, and is 420×120. It starts hidden. `platform/linux.rs` centers it at the top of the primary monitor's work area, then shows it.
- The size is locked with `minWidth/maxWidth/minHeight/maxHeight`, not `resizable: false`: GTK snaps non-resizable windows to the webview's natural size (420×200).
- Use `inner_size()`, not `outer_size()`, before the window is shown: tao reports an outer size of 0 until the first configure event.
- On a Wayland session, `platform/linux.rs` sets `GDK_BACKEND=x11` (XWayland), because GTK3 on native Wayland cannot do always-on-top, absolute positioning or skip-taskbar. Launching with `GDK_BACKEND` already set overrides this.
- The VS Code snap leaks `GTK_PATH`, `GIO_MODULE_DIR`, etc. into its terminal, which crashes WebKitGTK ("symbol lookup error … /snap/core20"). `platform/linux.rs` drops those variables when they point into `/snap/`.
- Design reference for the character: docs/design/kiko-orb.png. The app draws Kiko as SVG in code; never import this PNG in the app.
