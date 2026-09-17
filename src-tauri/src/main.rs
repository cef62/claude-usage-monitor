#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use claude_usage_monitor::poll::{self, Shared, Snapshot};
use claude_usage_monitor::tray;
use claude_usage_monitor::{alerts, log, settings};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_notification::NotificationExt;

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

/// Shows one macOS notification per newly crossed threshold. Failures are ignored: the title
/// marker still tells the story if notifications are denied.
fn notify_thresholds(app: &AppHandle, snapshot: &Snapshot) {
    let settings = app
        .state::<Arc<Mutex<settings::Settings>>>()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let now = poll::now();
    let due = {
        let state = app.state::<Mutex<alerts::AlertState>>();
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        alerts::evaluate(&mut state, &snapshot.quotas, &settings, now)
    };
    for alert in due {
        let _ = app
            .notification()
            .builder()
            .title(format!(
                "Claude usage: {} {}%",
                alert.label,
                alert.percent.round() as i64
            ))
            .body(format!(
                "Resets in {}",
                tray::countdown(alert.resets_at - now)
            ))
            .show();
        log::write(
            app,
            &format!(
                "alert {} {} at {}%",
                alert.key,
                alert.level,
                alert.percent.round() as i64
            ),
        );
    }
}

fn main() {
    let shared: Shared = Arc::new(Mutex::new(Snapshot::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
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
            let settings = Arc::new(Mutex::new(initial));
            app.manage(settings.clone());
            app.manage(Mutex::new(alerts::AlertState::default()));
            tray::setup(app.handle())?;
            log::write(
                app.handle(),
                &format!("startup v{}", app.package_info().version),
            );
            let handle = app.handle().clone();
            poll::run(shared, settings, log::path(app.handle()), move |snapshot| {
                tray::refresh_title(&handle, snapshot);
                notify_thresholds(&handle, snapshot);
                let _ = handle.emit("usage", snapshot);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
