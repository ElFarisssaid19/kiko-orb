//! Linux: window placement, display-server workarounds and owner-only files.

use std::{
    env, fs,
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

use tauri::{
    LogicalSize, LogicalUnit, Monitor, PhysicalPosition, PhysicalRect, PixelUnit, WebviewWindow,
    WindowSizeConstraints,
};

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

/// Centers the window horizontally at the top of the primary monitor's work area (just below the GNOME top bar).
/// The island draws its own gap from the top edge, so the window sits flush against it.
pub fn place_main_window(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = island_monitor(window)? else {
        return Ok(());
    };
    // Not `outer_size()`: tao only fills it after the first configure event, so it reads 0 while hidden.
    // The window is undecorated, so inner and outer sizes are the same.
    let width = window.inner_size()?.width;
    window.set_position(top_center(monitor.work_area(), width))
}

/// Resizes the window to `width` × `height` logical px and keeps it centered at the top of the work area.
pub fn set_island_size(window: &WebviewWindow, width: f64, height: f64) -> tauri::Result<()> {
    let Some(monitor) = island_monitor(window)? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let width = width.min(area.size.width as f64 / scale).round();
    let height = height.min(area.size.height as f64 / scale).round();

    // The size is locked with min = max (see tauri.conf.json): move both bounds in one request, then resize.
    let (w, h): (PixelUnit, PixelUnit) = (
        LogicalUnit::new(width).into(),
        LogicalUnit::new(height).into(),
    );
    window.set_size_constraints(WindowSizeConstraints {
        min_width: Some(w),
        min_height: Some(h),
        max_width: Some(w),
        max_height: Some(h),
    })?;
    window.set_size(LogicalSize::new(width, height))?;
    window.set_position(top_center(area, (width * scale).round() as u32))?;

    // Re-assert the overlay hints after the geometry change. None of these calls take focus.
    window.set_always_on_top(true)?;
    window.set_skip_taskbar(true)
}

fn island_monitor(window: &WebviewWindow) -> tauri::Result<Option<Monitor>> {
    Ok(window.primary_monitor()?.or(window.current_monitor()?))
}

/// Physical position of a `width`-px wide window centered at the top of `area`.
fn top_center(area: &PhysicalRect<i32, u32>, width: u32) -> PhysicalPosition<i32> {
    PhysicalPosition::new(
        area.position.x + (area.size.width as i32 - width as i32) / 2,
        area.position.y,
    )
}

pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

pub fn write_private_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    // `mode` only applies to a new file: tighten an existing one too, before writing the secret.
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(contents)
}

pub fn restrict_to_owner(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

/// Ubuntu and Debian, the systems Kiko targets first.
pub fn curl_install_command() -> &'static str {
    "sudo apt install curl"
}

#[cfg(test)]
pub fn is_owner_only(path: &Path) -> io::Result<bool> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o077 == 0)
}
