//! All OS-specific code lives here. The rest of the app only calls the functions below.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use tauri::WebviewWindow;

/// Process-level setup. Must be the first call in `run()`, before Tauri/GTK starts.
pub fn prepare_env() {
    #[cfg(target_os = "linux")]
    linux::prepare_env();
}

/// Moves the (still hidden) main window to its resting place on screen.
pub fn place_main_window(window: &WebviewWindow) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    return linux::place_main_window(window);

    #[cfg(not(target_os = "linux"))]
    {
        let _ = window;
        Ok(())
    }
}
