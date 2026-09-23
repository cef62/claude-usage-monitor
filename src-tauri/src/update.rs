//! In-app updates: daily check against the GitHub Release feed, one notification per version,
//! install on request from the tray menu. Delivery goes through tauri-plugin-updater.

use crate::log;
use crate::settings::Settings;
use crate::tray::UpdateItem;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::{Update, UpdaterExt};

/// Let the first usage poll finish before touching GitHub.
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);
/// How often the checker thread re-reads the clock while waiting for the next check.
const WAKE_POLL: Duration = Duration::from_secs(600);
const _: () = assert!(FIRST_CHECK_DELAY.as_secs() < CHECK_INTERVAL.as_secs());

/// Idle time between bytes before a check or download is abandoned. Without it a half-open
/// socket leaves `busy` set until the app restarts.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

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

fn auto_check_enabled(app: &AppHandle) -> bool {
    app.state::<Arc<Mutex<Settings>>>()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .auto_update_check
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

pub const FALLBACK_BODY: &str = "Right-click the tray icon → Help → Install update";
const SUMMARY_CHARS: usize = 120;

/// build-release.yml puts this bullet first in every release signed with the rotated key, so
/// 0.11.2 and older (which trust only the leaked key and show the first bullet) are told to
/// download by hand. Newer builds trust the new key and skip the bullet.
pub const LEGACY_KEY_NOTICE: &str = "On 0.11.2 or older?";

/// First changelog bullet of the release notes (`- <sha>: text` or `- text`), trimmed to fit a
/// notification; the menu hint when the notes carry no bullet.
pub fn notes_summary(body: Option<&str>) -> String {
    let bullet = body
        .unwrap_or("")
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("- "))
        .find(|l| !l.starts_with(LEGACY_KEY_NOTICE))
        .map(|l| match l.split_once(": ") {
            Some((sha, rest)) if sha.len() == 7 && sha.chars().all(|c| c.is_ascii_hexdigit()) => {
                rest
            }
            _ => l,
        })
        .map(str::trim)
        .filter(|l| !l.is_empty());
    let Some(text) = bullet else {
        return FALLBACK_BODY.to_string();
    };
    if text.chars().count() <= SUMMARY_CHARS {
        text.to_string()
    } else {
        let mut cut: String = text.chars().take(SUMMARY_CHARS - 1).collect();
        cut.push('…');
        cut
    }
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

/// First check after `FIRST_CHECK_DELAY`, then every `CHECK_INTERVAL`. `check` itself is
/// release builds only, so this just needs to exist without hammering the feed in dev.
pub fn spawn_checker(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        loop {
            // The loop keeps running while auto-check is off so re-enabling needs no restart.
            if auto_check_enabled(&app) {
                check(&app, Trigger::Auto);
            }
            // Short naps instead of one long sleep: a laptop asleep for a day would otherwise
            // push the next check a day further out.
            let due = std::time::Instant::now() + CHECK_INTERVAL;
            while std::time::Instant::now() < due {
                std::thread::sleep(WAKE_POLL);
            }
        }
    });
}

/// Release builds only: a debug build never checks or installs, so `tauri dev` can't arm an
/// update and rename its own `target/debug` out from under itself.
pub fn check(app: &AppHandle, trigger: Trigger) {
    let manual = trigger == Trigger::Manual;
    if cfg!(debug_assertions) {
        if manual {
            notify(app, "Updates are disabled in debug builds", "");
        }
        return;
    }
    if !begin(app) {
        if manual {
            notify(app, "Update in progress…", "");
        }
        return;
    }
    // The lock is released here; the network call runs without it.
    let result = tauri::async_runtime::block_on(async {
        app.updater_builder()
            .configure_client(|c| c.read_timeout(READ_TIMEOUT))
            .build()?
            .check()
            .await
    });
    let mut st = state(app);
    st.busy = false;
    match result {
        Ok(Some(update)) => {
            let version = update.version.clone();
            let body = notes_summary(update.body.as_deref());
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
                    &body,
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
            drop(st);
            notify(app, "Update in progress…", "");
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
                let quarter = (received * 4 / total).min(4);
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
    fn install_item_text_names_the_version() {
        assert_eq!(install_item_text(None), "Install update…");
        assert_eq!(install_item_text(Some("0.8.1")), "Install update 0.8.1…");
    }

    #[test]
    fn notes_summary_takes_the_first_bullet() {
        let body = "## 0.8.2\n\n### Patch Changes\n\n- 1a2b3c4: Reset notification once per window.\n- 5d6e7f8: Second line.\n\nmacOS: open anyway.";
        assert_eq!(
            notes_summary(Some(body)),
            "Reset notification once per window."
        );
        assert_eq!(notes_summary(None), FALLBACK_BODY);
        assert_eq!(notes_summary(Some("no bullets here")), FALLBACK_BODY);
        let long = format!("- {}", "x".repeat(200));
        let out = notes_summary(Some(&long));
        assert_eq!(out.chars().count(), 120);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn notes_summary_skips_the_legacy_key_notice() {
        let body =
            format!("- {LEGACY_KEY_NOTICE} download it from GitHub.\n- abcdef1: Real change.");
        assert_eq!(notes_summary(Some(&body)), "Real change.");
    }
}
