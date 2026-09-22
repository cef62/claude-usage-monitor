#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use claude_usage_monitor::poll::{self, Shared, Snapshot};
use claude_usage_monitor::tray;
use claude_usage_monitor::{alerts, history, log, settings, update};
use std::sync::{Arc, Mutex};
use std::time::Instant;
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
    let window = app
        .get_webview_window("popover")
        .ok_or("no popover window")?;
    window
        .set_size(tauri::LogicalSize::new(
            tray::POPOVER_WIDTH,
            f64::from(height),
        ))
        .map_err(|e| e.to_string())?;
    // set_size keeps the top-left corner, so a taller popover above a Windows taskbar would grow
    // down over it; re-place it against the icon it was opened from.
    let last = app
        .try_state::<tray::LastTrayRect>()
        .and_then(|l| *l.0.lock().unwrap_or_else(|p| p.into_inner()));
    if let Some(rect) = last {
        tray::place(&app, &window, &rect);
    }
    Ok(())
}

#[tauri::command]
fn quit(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

#[derive(serde::Serialize)]
struct About {
    version: String,
    os: &'static str,
}

#[tauri::command]
fn get_about(app: AppHandle) -> Result<About, String> {
    Ok(About {
        version: app.package_info().version.to_string(),
        os: if cfg!(target_os = "macos") {
            "macOS"
        } else if cfg!(target_os = "windows") {
            "Windows"
        } else {
            "Linux"
        },
    })
}

#[tauri::command]
fn get_settings(
    state: State<'_, Arc<Mutex<settings::Settings>>>,
) -> Result<settings::PopoverSettings, String> {
    let s = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Ok(settings::PopoverSettings::from(&*s))
}

/// Shows one notification per newly crossed threshold, reset, or projected run-out; a run-out
/// due together with a threshold alert for the same quota rides along in that alert's body.
/// Failures are ignored: the title marker still tells the story if notifications are denied.
fn notify_thresholds(app: &AppHandle, snapshot: &Snapshot) {
    let settings = app
        .state::<Arc<Mutex<settings::Settings>>>()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let now = poll::now();
    let (resets, due, run_outs) = {
        let state = app.state::<Mutex<alerts::AlertState>>();
        let mut state = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Resets first: they belong to the window that just ended, thresholds to the new one.
        let resets = alerts::resets(&mut state, &snapshot.quotas, &settings);
        let due = alerts::evaluate(&mut state, &snapshot.quotas, &settings, now);
        let run_outs = alerts::run_outs(
            &mut state,
            &snapshot.quotas,
            &snapshot.forecast,
            &settings,
            now,
        );
        (resets, due, run_outs)
    };
    for r in &run_outs {
        log::write(
            app,
            &format!("forecast {} 100% in {}", r.key, tray::countdown(r.at - now)),
        );
    }
    let (merged, alone) = alerts::partition_run_outs(&due, run_outs);
    for reset in resets {
        let _ = app
            .notification()
            .builder()
            .title(format!("Claude usage: {} reset", reset.label))
            .body(format!(
                "Back to 0% · next reset in {}",
                tray::countdown(reset.resets_at - now)
            ))
            .show();
        log::write(app, &format!("reset {}", reset.key));
    }
    for alert in due {
        let resets_in = tray::countdown(alert.resets_at - now);
        let body = match merged.iter().find(|r| r.key == alert.key) {
            Some(r) => format!(
                "Resets in {resets_in} · at this pace 100% in ~{}",
                tray::countdown(r.at - now)
            ),
            None => format!("Resets in {resets_in}"),
        };
        let _ = app
            .notification()
            .builder()
            .title(format!(
                "Claude usage: {} {}%",
                alert.label,
                alert.percent.round() as i64
            ))
            .body(body)
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
    for r in alone {
        let _ = app
            .notification()
            .builder()
            .title(format!("Claude usage: {}", r.label))
            .body(format!(
                "At this pace: 100% in ~{} · resets in {}",
                tray::countdown(r.at - now),
                tray::countdown(r.resets_at - now)
            ))
            .show();
    }
}

fn main() {
    let shared: Shared = Arc::new(Mutex::new(Snapshot::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            hide_popover,
            resize_popover,
            quit,
            get_settings,
            get_about
        ])
        .on_window_event(|window, event| {
            // Only the popover auto-hides on blur; any future window keeps its focus behaviour.
            if window.label() == "popover" && matches!(event, WindowEvent::Focused(false)) {
                let _ = window.hide();
                if let Some(hidden) = window.app_handle().try_state::<tray::HiddenAt>() {
                    *hidden.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
                }
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
            app.manage(tray::HiddenAt(Mutex::new(None)));
            app.manage(Mutex::new(update::UpdateState::default()));
            app.manage(tray::LastTrayRect(Mutex::new(None)));
            tray::setup(app.handle())?;
            update::spawn_checker(app.handle().clone());
            log::write(
                app.handle(),
                &format!("startup v{}", app.package_info().version),
            );
            let handle = app.handle().clone();
            poll::run(
                shared,
                settings,
                log::path(app.handle()),
                history::path(app.handle()),
                move |snapshot| {
                    tray::refresh(&handle, snapshot);
                    notify_thresholds(&handle, snapshot);
                    let _ = handle.emit("usage", snapshot);
                },
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
