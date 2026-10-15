//! The channel's only OS-specific code. Kiko's own lives in `src-tauri/src/platform/`; this crate
//! can't depend on the app (it would pull in Tauri and GTK).

/// The process that started this one: Claude Code. 0 when the OS gives no simple way to know.
pub fn parent_pid() -> u32 {
    #[cfg(unix)]
    return std::os::unix::process::parent_id();

    #[cfg(not(unix))]
    0
}
