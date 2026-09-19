//! In-app updates: daily check against the GitHub Release feed, one notification per version,
//! install on request from the tray menu. Delivery goes through tauri-plugin-updater.

use crate::log;
use crate::tray::UpdateItem;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::{Update, UpdaterExt};

/// Let the first usage poll finish before touching GitHub.
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);
const _: () = assert!(FIRST_CHECK_DELAY.as_secs() < CHECK_INTERVAL.as_secs());

/// What the last check found. The plugin's `Update` handle is kept so "Install" needs no
/// second round-trip to the feed.
#[derive(Default)]
pub struct UpdateState {
    pub available: Option<Update>,
    /// Version already announced by a notification; announce each version once.
    pub notified: Option<String>,
    /// A check or an install is running; a second request is dropped.
    pub busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trigger {
    Auto,
    Manual,
}

pub fn should_notify(version: &str, notified: Option<&str>) -> bool {
    notified != Some(version)
}

fn state(app: &AppHandle) -> MutexGuard<'_, UpdateState> {
    app.state::<Mutex<UpdateState>>()
        .inner()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let mut n = app.notification().builder().title(title);
    if !body.is_empty() {
        n = n.body(body);
    }
    let _ = n.show();
}

pub fn install_item_text(version: Option<&str>) -> String {
    match version {
        Some(v) => format!("Install update {v}…"),
        None => "Install update…".to_string(),
    }
}

/// Mirrors the stored update onto the Help menu item.
pub fn set_install_item(app: &AppHandle, version: Option<&str>) {
    if let Some(item) = app.try_state::<UpdateItem>() {
        let _ = item.0.set_text(install_item_text(version));
        let _ = item.0.set_enabled(version.is_some());
    }
}

/// Claims the busy flag; `false` means another check/install is already running.
fn begin(app: &AppHandle) -> bool {
    let mut st = state(app);
    if st.busy {
        return false;
    }
    st.busy = true;
    true
}

/// Release builds only: first check after `FIRST_CHECK_DELAY`, then every `CHECK_INTERVAL`.
pub fn spawn_checker(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        loop {
            check(&app, Trigger::Auto);
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

pub fn check(app: &AppHandle, trigger: Trigger) {
    let manual = trigger == Trigger::Manual;
    if !begin(app) {
        if manual {
            notify(app, "Already checking…", "");
        }
        return;
    }
    // The lock is released here; the network call runs without it.
    let result = tauri::async_runtime::block_on(async { app.updater()?.check().await });
    let mut st = state(app);
    st.busy = false;
    match result {
        Ok(Some(update)) => {
            let version = update.version.clone();
            st.available = Some(update);
            let announce = manual || should_notify(&version, st.notified.as_deref());
            if announce {
                st.notified = Some(version.clone());
            }
            drop(st);
            set_install_item(app, Some(&version));
            if announce {
                notify(
                    app,
                    &format!("Claude Usage Monitor {version} available"),
                    "Right-click the tray icon → Help → Install update",
                );
            }
            log::write(app, &format!("update available {version}"));
        }
        Ok(None) => {
            st.available = None;
            drop(st);
            set_install_item(app, None);
            if manual {
                let current = app.package_info().version.to_string();
                notify(app, &format!("Up to date ({current})"), "");
            }
            log::write(app, "update none");
        }
        Err(e) => {
            drop(st);
            if manual {
                notify(app, "Update check failed", &e.to_string());
            }
            log::write(app, &format!("update check failed {e}"));
        }
    }
}

/// Downloads and installs the stored update, then relaunches. On Windows the NSIS installer
/// ends the process itself; `restart` is still reached only on macOS.
pub fn install(app: &AppHandle) {
    let update = {
        let mut st = state(app);
        if st.busy {
            return;
        }
        let Some(update) = st.available.clone() else {
            return;
        };
        st.busy = true;
        update
    };
    let version = update.version.clone();
    log::write(app, &format!("update installing {version}"));
    let mut received: u64 = 0;
    let mut last_quarter: u64 = 0;
    let result = tauri::async_runtime::block_on(update.download_and_install(
        |chunk, total| {
            received += chunk as u64;
            if let Some(total) = total.filter(|t| *t > 0) {
                let quarter = received * 4 / total;
                if quarter > last_quarter {
                    last_quarter = quarter;
                    log::write(app, &format!("update download {}%", quarter * 25));
                }
            }
        },
        || {},
    ));
    state(app).busy = false;
    match result {
        Ok(()) => {
            log::write(app, &format!("update installed {version}, restarting"));
            app.restart();
        }
        Err(e) => {
            state(app).available = None;
            set_install_item(app, None);
            notify(app, "Update failed", &e.to_string());
            log::write(app, &format!("update install failed {e}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifies_each_version_once() {
        assert!(should_notify("0.8.0", None));
        assert!(!should_notify("0.8.0", Some("0.8.0")));
        assert!(should_notify("0.8.1", Some("0.8.0")));
    }

    #[test]
    fn first_check_comes_before_the_interval() {
        assert!(FIRST_CHECK_DELAY < CHECK_INTERVAL);
    }

    #[test]
    fn install_item_text_names_the_version() {
        assert_eq!(install_item_text(None), "Install update…");
        assert_eq!(install_item_text(Some("0.8.1")), "Install update 0.8.1…");
    }
}
