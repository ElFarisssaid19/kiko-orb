mod platform;

use tauri::{Manager, WebviewWindow};

/// Resizes the island window. Called by the frontend whenever the island changes shape.
#[tauri::command]
fn set_island_size(window: WebviewWindow, width: f64, height: f64) -> Result<(), String> {
    platform::set_island_size(&window, width, height).map_err(|e| e.to_string())
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
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![set_island_size])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
