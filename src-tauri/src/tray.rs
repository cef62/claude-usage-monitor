//! Tray icon, its title text, the right-click menu, and popover placement.

use crate::alerts;
use crate::log;
use crate::poll::{self, Snapshot, Status};
use crate::settings::{
    self, format_levels, parse_levels, PopoverSettings, Settings, INTERVAL_PRESETS, KEYS,
    SESSION_LEVEL_PRESETS, WEEKLY_LEVEL_PRESETS,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Rect, Wry};
use tauri_plugin_notification::NotificationExt;

pub const POPOVER_WIDTH: f64 = 320.0;
pub const TRAY_ID: &str = "main";
const USAGE_URL: &str = "https://claude.ai/settings/usage";
const POPOVER_GAP: f64 = 6.0;
pub const SESSION_GLYPH: &str = "◷";
pub const WEEKLY_GLYPH: &str = "▦";
pub const SEPARATOR: &str = "  ·  ";

/// Check items of the "Menu bar" submenu, kept so the handler can re-sync check marks.
pub struct MenuItems(pub HashMap<String, CheckMenuItem<Wry>>);

const DISPLAY_LABELS: [(&str, &str); 5] = [
    ("session", "Session"),
    ("weekly", "Weekly"),
    ("glyph", "Glyphs"),
    ("percent", "Percent"),
    ("remaining", "Remaining time"),
];

const ALERT_LABELS: [(&str, &str); 2] = [("alert_session", "Session"), ("alert_weekly", "Weekly")];

const POPOVER_LABELS: [(&str, &str); 3] = [
    ("show_time_ticks", "Time ticks"),
    ("show_elapsed_marker", "Elapsed marker"),
    ("show_threshold_marks", "Threshold marks"),
];

#[derive(Debug, PartialEq)]
pub enum MenuAction {
    Open,
    Quit,
    Toggle(String),
    Levels { key: String, levels: Vec<u8> },
    Interval(u64),
    TestNotification,
    OpenLog,
}

pub fn radio_id_levels(key: &str, levels: &[u8]) -> String {
    let joined = levels
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!("levels:{key}:{joined}")
}

pub fn radio_id_interval(secs: u64) -> String {
    format!("interval:{secs}")
}

pub fn parse_menu_id(id: &str) -> Option<MenuAction> {
    match id {
        "open" => return Some(MenuAction::Open),
        "quit" => return Some(MenuAction::Quit),
        "test-notification" => return Some(MenuAction::TestNotification),
        "open-log" => return Some(MenuAction::OpenLog),
        _ => {}
    }
    if let Some(key) = id.strip_prefix("set:") {
        return Some(MenuAction::Toggle(key.to_string()));
    }
    if let Some(rest) = id.strip_prefix("levels:") {
        let (key, raw) = rest.split_once(':')?;
        return parse_levels(raw).map(|levels| MenuAction::Levels {
            key: key.to_string(),
            levels,
        });
    }
    if let Some(raw) = id.strip_prefix("interval:") {
        return raw.parse().ok().map(MenuAction::Interval);
    }
    None
}

fn check_submenu(
    app: &AppHandle,
    text: &str,
    labels: &[(&str, &str)],
    current: &Settings,
    items: &mut HashMap<String, CheckMenuItem<Wry>>,
) -> tauri::Result<tauri::menu::Submenu<Wry>> {
    let mut submenu = SubmenuBuilder::new(app, text);
    for (key, label) in labels {
        let item = CheckMenuItem::with_id(
            app,
            format!("set:{key}"),
            *label,
            true,
            current.get(key),
            None::<&str>,
        )?;
        submenu = submenu.item(&item);
        items.insert((*key).to_string(), item);
    }
    submenu.build()
}

fn radio_submenu(
    app: &AppHandle,
    text: &str,
    entries: &[(String, String, bool)], // (id, label, checked)
    items: &mut HashMap<String, CheckMenuItem<Wry>>,
) -> tauri::Result<tauri::menu::Submenu<Wry>> {
    let mut submenu = SubmenuBuilder::new(app, text);
    for (id, label, checked) in entries {
        let item = CheckMenuItem::with_id(
            app,
            id.clone(),
            label.as_str(),
            true,
            *checked,
            None::<&str>,
        )?;
        submenu = submenu.item(&item);
        items.insert(id.clone(), item);
    }
    submenu.build()
}

fn level_entries(key: &str, presets: &[&[u8]], current: &[u8]) -> Vec<(String, String, bool)> {
    presets
        .iter()
        .map(|p| (radio_id_levels(key, p), format_levels(p), *p == current))
        .collect()
}

fn interval_entries(current: u64) -> Vec<(String, String, bool)> {
    INTERVAL_PRESETS
        .iter()
        .map(|&s| {
            (
                radio_id_interval(s),
                format!("{} min", s / 60),
                s == current,
            )
        })
        .collect()
}

pub fn countdown(secs: i64) -> String {
    if secs < 60 {
        return "<1m".to_string();
    }
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{mins:02}m")
    } else {
        format!("{mins}m")
    }
}

fn half(s: &Snapshot, key: &str, glyph: &str, now: i64, settings: &Settings) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(3);
    if settings.glyph {
        parts.push(glyph.to_string());
    }
    match s.quotas.iter().find(|q| q.key == key) {
        Some(q) => {
            if settings.percent {
                parts.push(format!("{}%", q.percent.round() as i64));
            }
            if settings.remaining {
                parts.push(format!("↻{}", countdown(q.resets_at - now)));
            }
        }
        None => parts.push("—".to_string()),
    }
    parts.join(" ")
}

fn status_text(text: &str, settings: &Settings) -> String {
    if settings.glyph {
        format!(" {SESSION_GLYPH} {text}")
    } else {
        format!(" {text}")
    }
}

pub fn title(s: &Snapshot, now: i64, settings: &Settings) -> String {
    let numbers = || {
        let mut halves = Vec::with_capacity(2);
        if settings.session {
            halves.push(half(s, "session", SESSION_GLYPH, now, settings));
        }
        if settings.weekly {
            halves.push(half(s, "weekly", WEEKLY_GLYPH, now, settings));
        }
        let warn = if alerts::marker(&s.quotas, settings) {
            "⚠ "
        } else {
            ""
        };
        format!(" {warn}{}", halves.join(SEPARATOR))
    };
    match s.status {
        Status::Ok => numbers(),
        Status::RateLimited { .. } => format!("{} (429)", numbers()),
        Status::NoToken => status_text("—", settings),
        Status::AuthExpired => status_text("! login", settings),
        Status::Error { .. } => status_text("! err", settings),
    }
}

/// Top-left corner for a popover of `width` logical pixels centered under the tray icon.
pub fn popover_origin(rect: &Rect, scale: f64, width: f64) -> LogicalPosition<f64> {
    let pos = rect.position.to_logical::<f64>(scale);
    let size = rect.size.to_logical::<f64>(scale);
    LogicalPosition::new(
        pos.x + size.width / 2.0 - width / 2.0,
        pos.y + size.height + POPOVER_GAP,
    )
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let current = lock_settings(app).clone();
    let open = MenuItemBuilder::with_id("open", "Open usage page").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let mut items = HashMap::new();
    let display = check_submenu(app, "Menu bar", &DISPLAY_LABELS, &current, &mut items)?;
    let popover = check_submenu(app, "Popover", &POPOVER_LABELS, &current, &mut items)?;

    let session_levels = radio_submenu(
        app,
        "Session levels",
        &level_entries("session", &SESSION_LEVEL_PRESETS, &current.session_levels),
        &mut items,
    )?;
    let weekly_levels = radio_submenu(
        app,
        "Weekly levels",
        &level_entries("weekly", &WEEKLY_LEVEL_PRESETS, &current.weekly_levels),
        &mut items,
    )?;
    let mut alerts_menu = SubmenuBuilder::new(app, "Alerts");
    for (key, label) in ALERT_LABELS {
        let item = CheckMenuItem::with_id(
            app,
            format!("set:{key}"),
            label,
            true,
            current.get(key),
            None::<&str>,
        )?;
        alerts_menu = alerts_menu.item(&item);
        items.insert(key.to_string(), item);
    }
    let test_item =
        MenuItemBuilder::with_id("test-notification", "Send test notification").build(app)?;
    let alerts_menu = alerts_menu
        .separator()
        .item(&session_levels)
        .item(&weekly_levels)
        .separator()
        .item(&test_item)
        .build()?;

    let interval = radio_submenu(
        app,
        "Check every",
        &interval_entries(current.poll_interval_secs),
        &mut items,
    )?;
    let open_log_item = MenuItemBuilder::with_id("open-log", "Open log").build(app)?;
    let help = SubmenuBuilder::new(app, "Help")
        .item(&open_log_item)
        .build()?;
    app.manage(MenuItems(items));

    let menu = MenuBuilder::new(app)
        .item(&open)
        .item(&display)
        .item(&popover)
        .item(&alerts_menu)
        .item(&interval)
        .item(&help)
        .separator()
        .item(&quit)
        .build()?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .title(title(&Snapshot::default(), poll::now(), &current))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match parse_menu_id(event.id().as_ref()) {
            Some(MenuAction::Open) => {
                let _ = tauri_plugin_opener::open_url(USAGE_URL, None::<&str>);
            }
            Some(MenuAction::Quit) => app.exit(0),
            Some(MenuAction::Toggle(key)) => on_setting_toggled(app, &key),
            Some(MenuAction::Levels { key, levels }) => on_levels_chosen(app, &key, &levels),
            Some(MenuAction::Interval(secs)) => on_interval_chosen(app, secs),
            Some(MenuAction::TestNotification) => send_test_notification(app),
            Some(MenuAction::OpenLog) => open_log(app),
            None => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_popover(tray.app_handle(), &rect);
            }
        })
        .build(app)?;
    Ok(())
}

fn lock_settings(app: &AppHandle) -> std::sync::MutexGuard<'_, Settings> {
    app.state::<Arc<Mutex<Settings>>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Re-syncs every check/radio mark, persists, logs (when `log_text` is `Some`), notifies the
/// popover, refreshes the title.
fn after_settings_change(app: &AppHandle, updated: &Settings, log_text: Option<&str>) {
    if let Some(items) = app.try_state::<MenuItems>() {
        for k in KEYS {
            if let Some(item) = items.0.get(k) {
                let _ = item.set_checked(updated.get(k));
            }
        }
        for p in SESSION_LEVEL_PRESETS {
            if let Some(item) = items.0.get(&radio_id_levels("session", p)) {
                let _ = item.set_checked(p == updated.session_levels.as_slice());
            }
        }
        for p in WEEKLY_LEVEL_PRESETS {
            if let Some(item) = items.0.get(&radio_id_levels("weekly", p)) {
                let _ = item.set_checked(p == updated.weekly_levels.as_slice());
            }
        }
        for s in INTERVAL_PRESETS {
            if let Some(item) = items.0.get(&radio_id_interval(s)) {
                let _ = item.set_checked(s == updated.poll_interval_secs);
            }
        }
    }
    if let Some(path) = settings::path(app) {
        // The in-memory value already applies; a failed write only loses persistence.
        let _ = settings::save(&path, updated);
    }
    if let Some(text) = log_text {
        log::write(app, text);
    }
    let _ = app.emit("settings", PopoverSettings::from(updated));
    refresh_title(app, &poll::read(&app.state::<poll::Shared>()));
}

fn on_setting_toggled(app: &AppHandle, key: &str) {
    let (changed, updated) = {
        let mut s = lock_settings(app);
        let changed = s.toggle(key);
        (changed, s.clone())
    };
    let log_text = changed.then(|| format!("settings {key}={}", updated.get(key)));
    after_settings_change(app, &updated, log_text.as_deref());
}

fn on_levels_chosen(app: &AppHandle, key: &str, levels: &[u8]) {
    let updated = {
        let mut s = lock_settings(app);
        s.set_levels(key, levels);
        s.clone()
    };
    after_settings_change(
        app,
        &updated,
        Some(&format!(
            "settings {key}_levels={}",
            format_levels(updated.levels(key))
        )),
    );
}

fn on_interval_chosen(app: &AppHandle, secs: u64) {
    let updated = {
        let mut s = lock_settings(app);
        s.set_poll_interval(secs);
        s.clone()
    };
    after_settings_change(
        app,
        &updated,
        Some(&format!(
            "settings poll_interval_secs={}",
            updated.poll_interval_secs
        )),
    );
}

fn send_test_notification(app: &AppHandle) {
    let _ = app
        .notification()
        .builder()
        .title("Claude usage: test")
        .body("Notifications are working")
        .show();
    log::write(app, "test notification");
}

fn open_log(app: &AppHandle) {
    let Some(path) = log::path(app) else {
        return;
    };
    if !path.exists() {
        let _ = log::append(&path, "log created from Help → Open log");
    }
    let _ = tauri_plugin_opener::reveal_item_in_dir(&path);
}

fn toggle_popover(app: &AppHandle, rect: &Rect) {
    let Some(window) = app.get_webview_window("popover") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let _ = window.set_position(popover_origin(rect, scale, POPOVER_WIDTH));
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn refresh_title(app: &AppHandle, s: &Snapshot) {
    let settings = lock_settings(app).clone();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(Some(title(s, poll::now(), &settings)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::usage::{Quota, SESSION_SECS, WEEKLY_SECS};
    use tauri::{LogicalSize, Position, Rect, Size};

    const NOW: i64 = 1_789_588_800;

    fn quota(key: &str, percent: f64, resets_in: i64, period: u64) -> Quota {
        Quota {
            key: key.to_string(),
            label: key.to_string(),
            percent,
            resets_at: NOW + resets_in,
            period_secs: period,
        }
    }

    fn snapshot(status: Status, quotas: Vec<Quota>) -> Snapshot {
        Snapshot {
            quotas,
            fetched_at: Some(NOW),
            next_poll_at: NOW + 180,
            status,
        }
    }

    fn both() -> Vec<Quota> {
        vec![
            quota("session", 48.4, 2 * 3600 + 13 * 60, SESSION_SECS),
            quota("weekly", 64.0, 3 * 86400 + 4 * 3600 + 20 * 60, WEEKLY_SECS),
            quota("weekly:fable", 12.0, 3 * 86400, WEEKLY_SECS),
        ]
    }

    #[test]
    fn countdown_formats() {
        assert_eq!(countdown(30), "<1m");
        assert_eq!(countdown(-5), "<1m");
        assert_eq!(countdown(42 * 60), "42m");
        assert_eq!(countdown(65 * 60), "1h05m");
        assert_eq!(countdown(2 * 3600 + 13 * 60), "2h13m");
        assert_eq!(countdown(3 * 86400 + 4 * 3600 + 20 * 60), "3d4h");
    }

    fn with(keys_off: &[&str]) -> Settings {
        let mut s = Settings::default();
        for k in keys_off {
            assert!(s.toggle(k), "could not disable {k}");
        }
        s
    }

    #[test]
    fn title_all_on_shows_both_halves_and_ignores_scoped() {
        let s = snapshot(Status::Ok, both());
        assert_eq!(
            title(&s, NOW, &Settings::default()),
            " ◷ 48% ↻2h13m  ·  ▦ 64% ↻3d4h"
        );
    }

    #[test]
    fn title_without_glyphs() {
        let s = snapshot(Status::Ok, both());
        assert_eq!(
            title(&s, NOW, &with(&["glyph"])),
            " 48% ↻2h13m  ·  64% ↻3d4h"
        );
    }

    #[test]
    fn title_single_half_has_no_separator() {
        let s = snapshot(Status::Ok, both());
        assert_eq!(title(&s, NOW, &with(&["weekly"])), " ◷ 48% ↻2h13m");
        assert_eq!(title(&s, NOW, &with(&["session"])), " ▦ 64% ↻3d4h");
    }

    #[test]
    fn title_percent_or_remaining_only() {
        let s = snapshot(Status::Ok, both());
        assert_eq!(title(&s, NOW, &with(&["remaining"])), " ◷ 48%  ·  ▦ 64%");
        assert_eq!(title(&s, NOW, &with(&["percent"])), " ◷ ↻2h13m  ·  ▦ ↻3d4h");
        assert_eq!(
            title(&s, NOW, &with(&["percent", "glyph", "weekly"])),
            " ↻2h13m"
        );
    }

    #[test]
    fn title_missing_quota_shows_dash() {
        let s = snapshot(Status::Ok, vec![quota("session", 48.0, 600, SESSION_SECS)]);
        assert_eq!(title(&s, NOW, &Settings::default()), " ◷ 48% ↻10m  ·  ▦ —");
        assert_eq!(title(&s, NOW, &with(&["glyph"])), " 48% ↻10m  ·  —");
    }

    #[test]
    fn title_by_status() {
        let d = Settings::default();
        let no_glyph = with(&["glyph"]);
        assert_eq!(title(&snapshot(Status::NoToken, vec![]), NOW, &d), " ◷ —");
        assert_eq!(
            title(&snapshot(Status::NoToken, vec![]), NOW, &no_glyph),
            " —"
        );
        assert_eq!(
            title(&snapshot(Status::AuthExpired, both()), NOW, &d),
            " ◷ ! login"
        );
        assert_eq!(
            title(&snapshot(Status::AuthExpired, both()), NOW, &no_glyph),
            " ! login"
        );
        assert_eq!(
            title(
                &snapshot(
                    Status::Error {
                        message: "x".into()
                    },
                    both()
                ),
                NOW,
                &d
            ),
            " ◷ ! err"
        );
        assert_eq!(
            title(
                &snapshot(Status::RateLimited { until: NOW + 900 }, both()),
                NOW,
                &d
            ),
            " ◷ 48% ↻2h13m  ·  ▦ 64% ↻3d4h (429)"
        );
    }

    #[test]
    fn title_marks_alerting_quota_at_95() {
        let s = snapshot(
            Status::Ok,
            vec![
                quota("session", 96.0, 2 * 3600 + 13 * 60, SESSION_SECS),
                quota("weekly", 64.0, 3 * 86400 + 4 * 3600 + 20 * 60, WEEKLY_SECS),
            ],
        );
        assert_eq!(
            title(&s, NOW, &Settings::default()),
            " ⚠ ◷ 96% ↻2h13m  ·  ▦ 64% ↻3d4h"
        );
        assert_eq!(
            title(
                &snapshot(Status::RateLimited { until: NOW + 900 }, s.quotas.clone()),
                NOW,
                &Settings::default()
            ),
            " ⚠ ◷ 96% ↻2h13m  ·  ▦ 64% ↻3d4h (429)"
        );
        assert_eq!(
            title(&s, NOW, &with(&["alert_session"])),
            " ◷ 96% ↻2h13m  ·  ▦ 64% ↻3d4h"
        );
    }

    #[test]
    fn title_marker_ignores_status_strings() {
        let s = snapshot(
            Status::AuthExpired,
            vec![quota("session", 99.0, 600, SESSION_SECS)],
        );
        assert_eq!(title(&s, NOW, &Settings::default()), " ◷ ! login");
    }

    #[test]
    fn menu_ids_round_trip() {
        assert_eq!(
            radio_id_levels("session", &[80, 95]),
            "levels:session:80,95"
        );
        assert_eq!(radio_id_interval(300), "interval:300");
        assert!(matches!(parse_menu_id("open"), Some(MenuAction::Open)));
        assert!(matches!(parse_menu_id("quit"), Some(MenuAction::Quit)));
        assert!(matches!(parse_menu_id("set:glyph"), Some(MenuAction::Toggle(k)) if k == "glyph"));
        assert!(matches!(
            parse_menu_id("levels:weekly:80,95"),
            Some(MenuAction::Levels { key, levels }) if key == "weekly" && levels == vec![80, 95]
        ));
        assert!(matches!(
            parse_menu_id("interval:600"),
            Some(MenuAction::Interval(600))
        ));
        assert!(matches!(
            parse_menu_id("test-notification"),
            Some(MenuAction::TestNotification)
        ));
        assert!(matches!(
            parse_menu_id("open-log"),
            Some(MenuAction::OpenLog)
        ));
        assert!(parse_menu_id("levels:weekly:x").is_none());
        assert!(parse_menu_id("interval:abc").is_none());
        assert!(parse_menu_id("bogus").is_none());
    }

    #[test]
    fn popover_is_centered_under_the_icon() {
        let rect = Rect {
            position: Position::Logical(LogicalPosition::new(1000.0, 0.0)),
            size: Size::Logical(LogicalSize::new(120.0, 22.0)),
        };
        let origin = popover_origin(&rect, 2.0, 320.0);
        assert_eq!(origin.x, 900.0);
        assert_eq!(origin.y, 28.0);
    }
}
