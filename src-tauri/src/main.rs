#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use claude_usage_monitor::poll::{self, Shared, Snapshot};
use claude_usage_monitor::settings;
use claude_usage_monitor::tray;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

#[tauri::command]
fn get_snapshot(state: State<'_, Shared>) -> Result<Snapshot, String> {
    Ok(poll::read(&state))
}

#[tauri::command]
fn hide_popover(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("popover")
        .ok_or("no popover window")?
        .hide()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn resize_popover(app: AppHandle, height: u32) -> Result<(), String> {
    app.get_webview_window("popover")
        .ok_or("no popover window")?
        .set_size(tauri::LogicalSize::new(
            tray::POPOVER_WIDTH,
            f64::from(height),
        ))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn quit(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

fn main() {
    let shared: Shared = Arc::new(Mutex::new(Snapshot::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            hide_popover,
            resize_popover,
            quit
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                let _ = window.hide();
            }
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let initial = settings::path(app.handle())
                .map(|p| settings::load(&p))
                .unwrap_or_default();
            app.manage(Mutex::new(initial));
            tray::setup(app.handle())?;
            let handle = app.handle().clone();
            poll::run(shared, move |snapshot| {
                tray::refresh_title(&handle, snapshot);
                let _ = handle.emit("usage", snapshot);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
