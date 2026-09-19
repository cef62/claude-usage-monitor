//! In-app updates: daily check against the GitHub Release feed, one notification per version,
//! install on request from the tray menu. Delivery goes through tauri-plugin-updater.

use std::time::Duration;
use tauri_plugin_updater::Update;

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

pub fn check(_app: &tauri::AppHandle, _trigger: Trigger) {}
pub fn install(_app: &tauri::AppHandle) {}

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
}
