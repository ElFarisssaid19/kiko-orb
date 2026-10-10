//! Linux: window placement and display-server workarounds.

use std::env;

use tauri::{PhysicalPosition, WebviewWindow};

/// Gap between the top of the work area (just below the GNOME top bar) and the island, in logical px.
const TOP_MARGIN: f64 = 8.0;

/// Exported by snap apps (e.g. the VS Code snap) to their child processes. They point GTK/GIO at
/// modules built against the snap's own glibc, which crashes Kiko at startup ("symbol lookup error")
/// when it is launched from that app's terminal.
const SNAP_LEAKED_VARS: &[&str] = &[
    "GTK_PATH",
    "GTK_EXE_PREFIX",
    "GTK_IM_MODULE_FILE",
    "GIO_MODULE_DIR",
    "GDK_PIXBUF_MODULEDIR",
    "GDK_PIXBUF_MODULE_FILE",
    "GSETTINGS_SCHEMA_DIR",
    "LOCPATH",
];

/// Must run before GTK is initialised.
///
/// GTK3 on native Wayland cannot keep a window above others, place it at fixed coordinates
/// or hide it from the taskbar, so on a Wayland session we run through XWayland instead.
/// Setting `GDK_BACKEND` yourself (e.g. `GDK_BACKEND=wayland`) skips this.
pub fn prepare_env() {
    // SAFETY (all env writes below): called at the very start of `run()`, before any other thread exists.
    for var in SNAP_LEAKED_VARS {
        if env::var(var).is_ok_and(|value| value.contains("/snap/")) {
            unsafe { env::remove_var(var) };
        }
    }

    let on_wayland = env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland")
        || env::var_os("WAYLAND_DISPLAY").is_some();

    if on_wayland && env::var_os("GDK_BACKEND").is_none() {
        unsafe { env::set_var("GDK_BACKEND", "x11") };
    }
}

/// Centers the window horizontally at the top of the primary monitor's work area.
pub fn place_main_window(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.primary_monitor()?.or(window.current_monitor()?) else {
        return Ok(());
    };

    let area = monitor.work_area();
    // Not `outer_size()`: tao only fills it after the first configure event, so it reads 0 while hidden.
    // The window is undecorated, so inner and outer sizes are the same.
    let size = window.inner_size()?;
    let x = area.position.x + (area.size.width as i32 - size.width as i32) / 2;
    let y = area.position.y + (TOP_MARGIN * monitor.scale_factor()).round() as i32;

    window.set_position(PhysicalPosition::new(x, y))
}
