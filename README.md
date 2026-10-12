# Kiko 🟣

**A floating desktop companion for Claude Code.**

Kiko is a lightweight desktop application that brings Claude Code sessions to a floating island at the top of your screen. Monitor agent activity, follow session progress, and respond to permission requests without constantly switching back to your terminal.
<img src="/kiko-orb/docs/asset/imgkiko.png" alt="Kiko floating island with animated character and speech bubble" width="600" />

Built with **Tauri 2, Rust, React, TypeScript, and Motion**, Kiko aims to make AI-assisted development more visible and interactive on Linux.

## ✨ Features

- **Floating Island:** An always-on-top window designed to stay out of your way.
- **Animated Character:** Kiko reacts to activity with expressive animations.
- **Six Moods:** Sleeping, Idle, Working, Needs You, Done, and Error.
- **Three Display States:** Hidden, Compact, and Expanded.
- **Session Overview:** View agent sessions, current activity, and recent steps.
- **Interactive Cards:** UI for permission requests and questions.
- **Smooth Animations:** Spring transitions and reduced-motion support.
- **Linux Support:** Designed first for Ubuntu with GNOME.

> **Current status:** The interface and floating island are implemented. The application currently uses development fixtures; the mock event stream and real Claude Code integration are planned next.

## 🖥️ How It Works

Kiko is designed around three main components:

1. **Claude Code integration:** Provides agent activity and pending requests.
2. **Event and state layer:** Processes events and updates the application store.
3. **Desktop interface:** Displays sessions, activity, and interactive requests in the floating island.

The current implementation uses development fixtures. A mock event source will be added before connecting real Claude Code hooks.

## 🎭 Character States

| Mood | Description |
|---|---|
| Sleeping | No active work to display |
| Idle | The agent is waiting or inactive |
| Working | The agent is processing a task |
| Needs You | User input or approval is required |
| Done | The task has completed |
| Error | An error needs attention |

## 🪟 Island Layout

| State | Size | Description |
|---|---|---|
| Hidden | 120 × 8 px | A thin pill at the top edge |
| Compact | 360 × 44 px | Character and short activity status |
| Expanded | 420 px wide | Session list, recent steps, and interactive cards |

The expanded height adjusts to its content.

## 🛠️ Tech Stack

| Technology | Purpose |
|---|---|
| Tauri 2 | Desktop application shell |
| Rust | Native window management and Linux-specific behavior |
| React | User interface |
| TypeScript | Application logic and type safety |
| Vite | Development server and production builds |
| Tailwind CSS 4 | Styling |
| Zustand | Application state management |
| Motion | Animations and transitions |
| WebKitGTK 4.1 | Linux web view |

## 🚀 Getting Started

### Prerequisites

- Linux with a supported desktop environment
- Node.js and npm
- Rust and Cargo
- Tauri 2 prerequisites
- WebKitGTK 4.1 and the required GTK development libraries

Ubuntu may require additional system packages for Tauri development.

### Installation

Clone the repository:

```bash
git clone <REPOSITORY_URL>
cd <REPOSITORY_DIRECTORY>
```

Install JavaScript dependencies:

```bash
npm install
```

Start the development application:

```bash
npm run tauri dev
```

The exact commands may vary depending on the repository's package scripts. Check `package.json` and the project documentation if a command is unavailable.

## 🧪 Development

Kiko uses a modular architecture so that the UI can consume events through a common interface.

The development workflow includes:

- React components for the character, island, speech bubble, and cards.
- Zustand for shared application state.
- An event source abstraction for incoming session events.
- Rust commands for native window resizing and positioning.
- Platform-specific code for Linux window-management workarounds.

Please follow the conventions documented in `CLAUDE.md` before making changes.

## 🗺️ Roadmap

- [x] Initialize the Tauri and React project.
- [x] Implement the animated Kiko character.
- [x] Build the floating island and its display states.
- [x] Add speech bubbles and interactive UI cards.
- [ ] Implement a scripted mock event stream.
- [ ] Integrate real Claude Code hooks.
- [ ] Send permission decisions and answers back to the agent.
- [ ] Test click behavior when other applications have focus.
- [ ] Improve click-through behavior around the island.
- [ ] Measure resource usage and optimize performance.
- [ ] Package a Linux installer (`.deb` or AppImage).
- [ ] Explore Windows and macOS support.

## 🤝 Contributing

Contributions, bug reports, and improvement ideas are welcome.

1. Fork the repository or obtain collaborator access.
2. Create a feature branch:

   ```bash
   git switch -c feature/your-feature
   ```

3. Make your changes and run the available checks.
4. Commit your work:

   ```bash
   git add .
   git commit -m "Add your feature"
   ```

5. Push your branch and open a Pull Request.

Keep changes focused, follow the existing architecture, and avoid modifying user-level Claude Code settings without explicit authorization.

## 🔒 Privacy and Permissions

Claude Code integration may require configuring hooks in the user's `~/.claude/settings.json`. Such changes should only happen with explicit user approval.

Do not expose API keys, private session data, or other sensitive information in logs or commits.

## 📄 License

Check the repository for its current license before redistributing or reusing the project.

---

**Kiko — Stay in your flow while your AI coding agent works.**
