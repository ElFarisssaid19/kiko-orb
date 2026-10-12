//! All OS-specific code lives here. The rest of the app only calls the functions below.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use std::{io, path::Path};

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

/// Creates `dir` (and its parents); the last one readable by its owner only.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    return linux::create_private_dir(dir);

    #[cfg(not(target_os = "linux"))]
    std::fs::create_dir_all(dir)
}

/// Writes a file only its owner can read (it holds the hook token).
pub fn write_private_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    return linux::write_private_file(path, contents);

    #[cfg(not(target_os = "linux"))]
    std::fs::write(path, contents)
}

/// Makes an existing file readable by its owner only.
pub fn restrict_to_owner(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    return linux::restrict_to_owner(path);

    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Ok(())
    }
}

/// The command that installs curl, shown when Kiko's hooks can't find it.
pub fn curl_install_command() -> &'static str {
    #[cfg(target_os = "linux")]
    return linux::curl_install_command();

    #[cfg(not(target_os = "linux"))]
    "your system's package manager"
}

/// Whether only the owner can read or write `path`.
#[cfg(test)]
pub fn is_owner_only(path: &Path) -> io::Result<bool> {
    #[cfg(target_os = "linux")]
    return linux::is_owner_only(path);

    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        Ok(true)
    }
}
