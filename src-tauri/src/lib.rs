mod chat;
mod hooks;
mod platform;

use std::{
    io,
    time::{Duration, SystemTime},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use chat::{ChatInstaller, ChatPreview, ChatState, ClaudeCli};
use hooks::{
    Action, AgentEvent, Applied, Emitted, HookLogEntry, HookServer, Installer, KikoConfig, Preview,
    UserResponse,
};

/// Tauri events sent to every window.
const AGENT_EVENT: &str = "kiko://agent-event";
const HOOK_LOG: &str = "kiko://hook-log";
/// Kiko's hooks were installed or removed.
const HOOKS_CHANGED: &str = "kiko://hooks-changed";
/// Kiko's chat was installed or removed.
const CHAT_CHANGED: &str = "kiko://chat-changed";

struct ServerError {
    port_busy: bool,
    message: String,
}

struct Kiko {
    config: KikoConfig,
    /// Installs Kiko's hooks into Claude Code's user settings.
    installer: Installer,
    /// Installs the chat channel for the sessions started with Kiko's launch command.
    chat: ChatInstaller,
    server: Result<HookServer, ServerError>,
}

impl Kiko {
    fn server(&self) -> Result<&HookServer, String> {
        self.server.as_ref().map_err(|error| error.message.clone())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HooksStatus {
    /// "installed" | "not-installed" | "port-busy" | "server-error"
    status: &'static str,
    /// Installed with the current hook commands (port, header file).
    up_to_date: bool,
    port: u16,
    settings_path: String,
    error: Option<String>,
    /// Why the hook commands can't run here (curl missing or too old), with the command to fix it.
    curl_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatStatus {
    /// "installed" | "not-installed" | "claude-missing"
    status: &'static str,
    #[serde(flatten)]
    state: ChatState,
    claude_path: Option<String>,
    /// e.g. "2.1.296"; `None` if `claude --version` didn't answer.
    claude_version: Option<String>,
    /// Older than the first Claude Code with channels.
    claude_too_old: bool,
    min_claude_version: String,
    /// `claude --mcp-config ~/.config/kiko/mcp.json --dangerously-load-development-channels server:kiko`
    launch_command: String,
    /// `alias kiko-claude='…'`, for the user to add to their shell.
    alias: String,
    binary_path: String,
    mcp_config_path: String,
    /// kiko-channel isn't built next to Kiko, and how to build it.
    source_error: Option<String>,
    error: Option<String>,
}

/// Resizes the island window. Called by the frontend whenever the island changes shape.
#[tauri::command]
fn set_island_size(window: WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    platform::set_island_size(&window, width, height).map_err(|e| e.to_string())
}

/// The user's answer to a held permission or question.
#[tauri::command]
fn respond_to_request(
    kiko: State<Kiko>,
    request_id: String,
    response: UserResponse,
) -> Result<(), String> {
    kiko.server()?.respond(&request_id, response)
}

/// Requests still waiting for the user, oldest first, for a window that just opened.
#[tauri::command]
fn pending_requests(kiko: State<Kiko>) -> Vec<AgentEvent> {
    kiko.server()
        .map(HookServer::pending_events)
        .unwrap_or_default()
}

#[tauri::command]
fn hook_log(kiko: State<Kiko>) -> Vec<HookLogEntry> {
    kiko.server().map(HookServer::log).unwrap_or_default()
}

#[tauri::command]
fn hooks_status(kiko: State<Kiko>) -> HooksStatus {
    let state = kiko.installer.state();
    let (status, up_to_date, error) = match (&kiko.server, state) {
        (Err(error), _) => (
            if error.port_busy {
                "port-busy"
            } else {
                "server-error"
            },
            false,
            Some(error.message.clone()),
        ),
        (Ok(_), Ok(state)) => (
            if state.installed {
                "installed"
            } else {
                "not-installed"
            },
            state.up_to_date,
            None,
        ),
        (Ok(_), Err(error)) => ("not-installed", false, Some(error.to_string())),
    };
    HooksStatus {
        status,
        up_to_date,
        port: kiko
            .server
            .as_ref()
            .map_or(kiko.config.port, HookServer::port),
        settings_path: kiko.installer.settings_path.display().to_string(),
        error,
        curl_error: kiko.installer.curl().err().map(|e| e.to_string()),
    }
}

/// What installing or removing Kiko's hooks would change, before anything is written.
#[tauri::command]
fn preview_hooks_change(kiko: State<Kiko>, action: Action) -> Result<Preview, String> {
    kiko.installer.preview(action).map_err(|e| e.to_string())
}

/// Applies a previewed change; refused if settings.json changed since the preview.
#[tauri::command]
fn apply_hooks_change(
    app: AppHandle,
    kiko: State<Kiko>,
    action: Action,
    fingerprint: String,
) -> Result<Applied, String> {
    let applied = kiko
        .installer
        .apply(action, &fingerprint, SystemTime::now())
        .map_err(|e| e.to_string())?;
    let _ = app.emit(HOOKS_CHANGED, ());
    Ok(applied)
}

/// Chat setup: installed or not, the Claude Code CLI and its version, the launch command.
/// Async: it runs `claude --version`, which must not hold the main thread.
#[tauri::command]
async fn chat_status(kiko: State<'_, Kiko>) -> Result<ChatStatus, String> {
    let installer = &kiko.chat;
    let claude =
        chat::find_claude(&installer.home, std::env::var_os("PATH").as_deref()).map(|path| {
            ClaudeCli {
                version: chat::claude_version(&path),
                path,
            }
        });
    let (state, error) = match installer.state() {
        Ok(state) => (state, None),
        Err(error) => (ChatState::default(), Some(error.to_string())),
    };
    let (major, minor, patch) = chat::MIN_CLAUDE_VERSION;
    Ok(ChatStatus {
        status: match (&claude, state.installed) {
            (None, _) => "claude-missing",
            (Some(_), true) => "installed",
            (Some(_), false) => "not-installed",
        },
        state,
        claude_path: claude.as_ref().map(|c| c.path.display().to_string()),
        claude_version: claude.as_ref().and_then(|c| c.version.clone()),
        claude_too_old: claude.as_ref().is_some_and(ClaudeCli::too_old),
        min_claude_version: format!("{major}.{minor}.{patch}"),
        launch_command: installer.launch_command(),
        alias: installer.alias(),
        binary_path: installer.display(&installer.binary_path),
        mcp_config_path: installer.display(&installer.mcp_config_path),
        source_error: installer.source_error(),
        error,
    })
}

/// Every file Kiko would create or remove to install or remove chat, before anything is written.
#[tauri::command]
async fn preview_chat_change(kiko: State<'_, Kiko>, action: Action) -> Result<ChatPreview, String> {
    kiko.chat.preview(action).map_err(|e| e.to_string())
}

/// Applies a previewed chat change; refused if the files changed since the preview.
#[tauri::command]
async fn apply_chat_change(
    app: AppHandle,
    kiko: State<'_, Kiko>,
    action: Action,
    fingerprint: String,
) -> Result<(), String> {
    kiko.chat
        .apply(action, &fingerprint)
        .map_err(|e| e.to_string())?;
    let _ = app.emit(CHAT_CHANGED, ());
    Ok(())
}

/// The user's chat message for a session's channel. `id` is the frontend's, shown at once.
#[tauri::command]
fn send_chat_message(
    kiko: State<Kiko>,
    session_id: String,
    id: String,
    text: String,
) -> Result<(), String> {
    kiko.server()?.send_chat(&session_id, id, text)
}

/// A `chat-linked` event per session that has a channel now, for a window that just opened.
#[tauri::command]
fn chat_links(kiko: State<Kiko>) -> Vec<AgentEvent> {
    kiko.server()
        .map(HookServer::chat_links)
        .unwrap_or_default()
}

/// Opens (or focuses) the settings window, or the debug playground in dev builds.
/// The frontend picks the page from the window label.
#[tauri::command]
async fn open_window(app: AppHandle, kind: String) -> Result<(), String> {
    let (label, title, width, height) = match kind.as_str() {
        "settings" => ("settings", "Kiko settings", 560.0, 720.0),
        "playground" if cfg!(debug_assertions) => ("playground", "Kiko playground", 1280.0, 840.0),
        _ => return Err(format!("no window named {kind}")),
    };
    if let Some(window) = app.get_webview_window(label) {
        return window.set_focus().map_err(|e| e.to_string());
    }
    WebviewWindowBuilder::new(&app, label, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(width, height)
        .center()
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn start_hook_server(app: &AppHandle, config: &KikoConfig) -> Result<HookServer, ServerError> {
    let handle = app.clone();
    HookServer::start(
        config.port,
        config.token.clone(),
        Duration::from_secs(config.decision_timeout_secs),
        move |emitted| {
            let _ = match emitted {
                Emitted::Event(event) => handle.emit(AGENT_EVENT, event),
                Emitted::Log(entry) => handle.emit(HOOK_LOG, entry),
            };
        },
    )
    .map_err(|error| ServerError {
        port_busy: error.kind() == io::ErrorKind::AddrInUse,
        message: if error.kind() == io::ErrorKind::AddrInUse {
            format!("Port {} is already in use by another program.", config.port)
        } else {
            error.to_string()
        },
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    platform::prepare_env();

    tauri::Builder::default()
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("main window is declared in tauri.conf.json");
            // The window starts hidden so it never flashes at the default position.
            platform::place_main_window(&window)?;
            window.show()?;

            let home = app.path().home_dir()?;
            let config_path = app.path().config_dir()?.join("kiko").join("config.json");
            // Also writes the auth-header file the hook commands read the token from.
            let config = KikoConfig::load_or_create(&config_path)?;
            let installer = Installer {
                settings_path: hooks::claude_settings_path(
                    &home,
                    std::env::var("CLAUDE_CONFIG_DIR").ok().as_deref(),
                ),
                record_path: config_path.with_file_name("hooks-install.json"),
                port: config.port,
                auth_header: hooks::shell_path(&hooks::auth_header_path(&config_path), &home),
                curl: "curl".into(),
            };
            let channel = format!("kiko-channel{}", std::env::consts::EXE_SUFFIX);
            let chat = ChatInstaller {
                // Built next to Kiko: `cargo build` builds both (see Cargo.toml).
                source: std::env::current_exe()?.with_file_name(&channel),
                binary_path: app
                    .path()
                    .data_dir()?
                    .join("kiko")
                    .join("bin")
                    .join(&channel),
                mcp_config_path: config_path.with_file_name("mcp.json"),
                kiko_config_path: config_path.clone(),
                record_path: config_path.with_file_name("chat-install.json"),
                home,
            };
            let server = start_hook_server(app.handle(), &config);
            app.manage(Kiko {
                config,
                installer,
                chat,
                server,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_island_size,
            respond_to_request,
            pending_requests,
            hook_log,
            hooks_status,
            preview_hooks_change,
            apply_hooks_change,
            chat_status,
            preview_chat_change,
            apply_chat_change,
            send_chat_message,
            chat_links,
            open_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
