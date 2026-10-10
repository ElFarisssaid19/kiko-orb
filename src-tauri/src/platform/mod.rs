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

/// Resizes the island window (logical px) and keeps it centered at the top of the screen,
/// still transparent, on top, out of the taskbar and without taking focus.
pub fn set_island_size(window: &WebviewWindow, width: f64, height: f64) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    return linux::set_island_size(window, width, height);

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, width, height);
        Ok(())
    }
}
