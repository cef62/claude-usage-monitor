//! Tray icon, its title text, the right-click menu, and popover placement.

use crate::alerts;
#[cfg(not(target_os = "macos"))]
use crate::icon;
use crate::log;
use crate::poll::{self, Snapshot, Status};
use crate::settings::{
    self, format_levels, parse_levels, PopoverSettings, Settings, INTERVAL_PRESETS, KEYS,
    SESSION_LEVEL_PRESETS, WEEKLY_LEVEL_PRESETS,
};
use crate::update;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Rect, WebviewWindow, Wry};
use tauri_plugin_autostart::ManagerExt;
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

/// macOS has a menu bar; every other desktop calls it the tray.
#[cfg(target_os = "macos")]
const DISPLAY_MENU_LABEL: &str = "Menu bar";
/// macOS has a menu bar; every other desktop calls it the tray.
#[cfg(not(target_os = "macos"))]
const DISPLAY_MENU_LABEL: &str = "Tray";

const ALERT_LABELS: [(&str, &str); 3] = [
    ("alert_session", "Session"),
    ("alert_weekly", "Weekly"),
    ("alert_reset", "Notify on reset"),
];

const POPOVER_LABELS: [(&str, &str); 4] = [
    ("show_time_ticks", "Time ticks"),
    ("show_elapsed_marker", "Elapsed marker"),
    ("show_threshold_marks", "Threshold marks"),
    ("show_history", "History line"),
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
    Autostart,
    CheckUpdates,
    InstallUpdate,
    About,
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
        "autostart" => return Some(MenuAction::Autostart),
        "check-updates" => return Some(MenuAction::CheckUpdates),
        "install-update" => return Some(MenuAction::InstallUpdate),
        "about" => return Some(MenuAction::About),
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

/// A monitor's area in logical pixels, with its global origin: the tray rect is in global screen
/// coordinates, so a second display to the right or below has a non-zero origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Top-left corner for a `width`×`height` logical-pixel popover next to the tray icon: centred
/// below it when the icon is in the top half of the monitor (menu bar), centred above it
/// otherwise (bottom taskbar), and never past the monitor's edges.
pub fn popover_origin(
    rect: &Rect,
    scale: f64,
    width: f64,
    height: f64,
    monitor: Area,
) -> LogicalPosition<f64> {
    let pos = rect.position.to_logical::<f64>(scale);
    let icon = rect.size.to_logical::<f64>(scale);
    let x = (pos.x + icon.width / 2.0 - width / 2.0).clamp(
        monitor.x,
        (monitor.x + monitor.width - width).max(monitor.x),
    );
    let y = if pos.y + icon.height / 2.0 < monitor.y + monitor.height / 2.0 {
        pos.y + icon.height + POPOVER_GAP
    } else {
        pos.y - POPOVER_GAP - height
    };
    LogicalPosition::new(x, y.max(monitor.y))
}

/// Clicking the tray icon on Windows first steals focus from the popover, which hides it, and
/// then delivers the click, which would show it again. Ignore shows this soon after a blur-hide.
pub const BLUR_GUARD: Duration = Duration::from_millis(400);

/// When the popover was last hidden because it lost focus.
pub struct HiddenAt(pub Mutex<Option<Instant>>);

/// The tray rect of the last click, so a later resize can re-place the popover next to it.
pub struct LastTrayRect(pub Mutex<Option<Rect>>);

/// The "Install update…" item: text and enabled state follow `update::UpdateState`.
pub struct UpdateItem(pub MenuItem<Wry>);

pub fn blur_guard_active(hidden_at: Option<Instant>, now: Instant) -> bool {
    hidden_at.is_some_and(|t| now.duration_since(t) < BLUR_GUARD)
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let current = lock_settings(app).clone();
    let open = MenuItemBuilder::with_id("open", "Open usage page").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let mut items = HashMap::new();
    let display = check_submenu(
        app,
        DISPLAY_MENU_LABEL,
        &DISPLAY_LABELS,
        &current,
        &mut items,
    )?;
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
    // The OS login-item registry is the only source of truth: nothing is persisted in settings.
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start at login",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    items.insert("autostart".to_string(), autostart.clone());
    let open_log_item = MenuItemBuilder::with_id("open-log", "Open log").build(app)?;
    let check_updates =
        MenuItemBuilder::with_id("check-updates", "Check for updates…").build(app)?;
    let install_update = MenuItemBuilder::with_id("install-update", "Install update…")
        .enabled(false)
        .build(app)?;
    app.manage(UpdateItem(install_update.clone()));
    let auto_check = CheckMenuItem::with_id(
        app,
        "set:auto_update_check",
        "Check for updates automatically",
        true,
        current.get("auto_update_check"),
        None::<&str>,
    )?;
    items.insert("auto_update_check".to_string(), auto_check.clone());
    let about = MenuItemBuilder::with_id("about", "About…").build(app)?;
    let help = SubmenuBuilder::new(app, "Help")
        .item(&open_log_item)
        .separator()
        .item(&auto_check)
        .item(&check_updates)
        .item(&install_update)
        .separator()
        .item(&about)
        .build()?;
    app.manage(MenuItems(items));

    let menu = MenuBuilder::new(app)
        .item(&open)
        .item(&display)
        .item(&popover)
        .item(&alerts_menu)
        .item(&interval)
        .item(&autostart)
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
            Some(MenuAction::Autostart) => on_autostart_toggled(app),
            Some(MenuAction::CheckUpdates) => {
                let app = app.clone();
                std::thread::spawn(move || update::check(&app, update::Trigger::Manual));
            }
            Some(MenuAction::InstallUpdate) => {
                let app = app.clone();
                std::thread::spawn(move || update::install(&app));
            }
            Some(MenuAction::About) => show_about(app),
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
    #[cfg(not(target_os = "macos"))]
    refresh(app, &Snapshot::default());
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
    refresh(app, &poll::read(&app.state::<poll::Shared>()));
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

/// Flips the OS login item, then shows whatever the OS reports so a failed call never lies.
fn on_autostart_toggled(app: &AppHandle) {
    let launcher = app.autolaunch();
    let result = if launcher.is_enabled().unwrap_or(false) {
        launcher.disable()
    } else {
        launcher.enable()
    };
    if let Err(e) = result {
        log::write(app, &format!("autostart error {e}"));
    }
    let enabled = launcher.is_enabled().unwrap_or(false);
    if let Some(items) = app.try_state::<MenuItems>() {
        if let Some(item) = items.0.get("autostart") {
            let _ = item.set_checked(enabled);
        }
    }
    log::write(app, &format!("settings autostart={enabled}"));
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

/// Opens the popover (where it was last placed) and switches it to the About view.
fn show_about(app: &AppHandle) {
    let Some(window) = app.get_webview_window("popover") else {
        return;
    };
    let last = app
        .try_state::<LastTrayRect>()
        .and_then(|l| *l.0.lock().unwrap_or_else(|p| p.into_inner()));
    if let Some(rect) = last {
        place(app, &window, &rect);
    }
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit("show-about", ());
}

fn toggle_popover(app: &AppHandle, rect: &Rect) {
    let Some(window) = app.get_webview_window("popover") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    let hidden_at = app
        .try_state::<HiddenAt>()
        .and_then(|h| *h.0.lock().unwrap_or_else(|p| p.into_inner()));
    if blur_guard_active(hidden_at, Instant::now()) {
        return;
    }
    if let Some(last) = app.try_state::<LastTrayRect>() {
        *last.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(*rect);
    }
    place(app, &window, rect);
    let _ = window.show();
    let _ = window.set_focus();
}

/// The monitor whose physical rectangle contains the icon's centre. The hidden popover's own
/// monitor is not it on a multi-display Mac, and `monitor_from_point` takes logical points on
/// macOS but physical pixels on Windows, so containment is checked by hand.
fn monitor_under(app: &AppHandle, rect: &Rect) -> Option<tauri::Monitor> {
    app.available_monitors().ok()?.into_iter().find(|m| {
        let pos = rect.position.to_physical::<f64>(m.scale_factor());
        let size = rect.size.to_physical::<f64>(m.scale_factor());
        let (cx, cy) = (pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        let (mx, my) = (f64::from(m.position().x), f64::from(m.position().y));
        cx >= mx
            && cy >= my
            && cx < mx + f64::from(m.size().width)
            && cy < my + f64::from(m.size().height)
    })
}

/// Moves the popover next to the tray icon at `rect`; the window keeps its current size.
pub fn place(app: &AppHandle, window: &WebviewWindow, rect: &Rect) {
    let monitor = monitor_under(app, rect)
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let (scale, area) = match monitor {
        Some(m) => {
            let scale = m.scale_factor();
            let origin = m.position().to_logical::<f64>(scale);
            let size = m.size().to_logical::<f64>(scale);
            (
                scale,
                Area {
                    x: origin.x,
                    y: origin.y,
                    width: size.width,
                    height: size.height,
                },
            )
        }
        None => (
            window.scale_factor().unwrap_or(1.0),
            Area {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            },
        ),
    };
    let height = window
        .outer_size()
        .map(|s| s.to_logical::<f64>(scale).height)
        .unwrap_or(240.0);
    let _ = window.set_position(popover_origin(rect, scale, POPOVER_WIDTH, height, area));
}

/// Pushes the snapshot to the tray. macOS shows the text as the status-item title; Windows has
/// no title, so the same text becomes the tooltip and the numbers are drawn into the icon.
pub fn refresh(app: &AppHandle, s: &Snapshot) {
    let settings = lock_settings(app).clone();
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let text = title(s, poll::now(), &settings);
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(Some(text));
    #[cfg(not(target_os = "macos"))]
    {
        let _ = tray.set_tooltip(Some(text.trim()));
        let rgba = icon::render(s, &settings, poll::now());
        let image = tauri::image::Image::new_owned(rgba, icon::SIZE, icon::SIZE);
        let _ = tray.set_icon(Some(image));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::usage::{Quota, SESSION_SECS, WEEKLY_SECS};
    use std::time::{Duration, Instant};
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
            plan: None,
            extra: None,
            history: HashMap::new(),
            forecast: HashMap::new(),
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
        assert!(matches!(
            parse_menu_id("autostart"),
            Some(MenuAction::Autostart)
        ));
        assert!(matches!(
            parse_menu_id("check-updates"),
            Some(MenuAction::CheckUpdates)
        ));
        assert!(matches!(
            parse_menu_id("install-update"),
            Some(MenuAction::InstallUpdate)
        ));
        assert!(matches!(parse_menu_id("about"), Some(MenuAction::About)));
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

    const MONITOR: Area = Area {
        x: 0.0,
        y: 0.0,
        width: 1440.0,
        height: 900.0,
    };

    fn icon_rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            position: Position::Logical(LogicalPosition::new(x, y)),
            size: Size::Logical(LogicalSize::new(w, h)),
        }
    }

    #[test]
    fn popover_is_centered_under_a_menu_bar_icon() {
        let origin = popover_origin(
            &icon_rect(1000.0, 0.0, 120.0, 22.0),
            2.0,
            320.0,
            240.0,
            MONITOR,
        );
        assert_eq!(origin.x, 900.0);
        assert_eq!(origin.y, 28.0);
    }

    #[test]
    fn popover_sits_above_a_bottom_taskbar_icon() {
        let origin = popover_origin(
            &icon_rect(1200.0, 860.0, 24.0, 40.0),
            1.0,
            320.0,
            240.0,
            MONITOR,
        );
        assert_eq!(origin.x, 1052.0);
        assert_eq!(origin.y, 860.0 - 6.0 - 240.0);
    }

    #[test]
    fn popover_is_clamped_to_the_monitor_edges() {
        let right = popover_origin(
            &icon_rect(1406.0, 860.0, 24.0, 40.0),
            1.0,
            320.0,
            240.0,
            MONITOR,
        );
        assert_eq!(right.x, 1440.0 - 320.0);
        let left = popover_origin(
            &icon_rect(10.0, 0.0, 24.0, 22.0),
            1.0,
            320.0,
            240.0,
            MONITOR,
        );
        assert_eq!(left.x, 0.0);
    }

    #[test]
    fn popover_stays_on_a_monitor_to_the_right() {
        let monitor = Area {
            x: 1440.0,
            y: 0.0,
            width: 2560.0,
            height: 1440.0,
        };
        let origin = popover_origin(
            &icon_rect(3500.0, 0.0, 120.0, 22.0),
            1.0,
            320.0,
            240.0,
            monitor,
        );
        assert_eq!(origin.x, 3400.0);
        assert_eq!(origin.y, 28.0);
        let edge = popover_origin(
            &icon_rect(3990.0, 0.0, 120.0, 22.0),
            1.0,
            320.0,
            240.0,
            monitor,
        );
        assert_eq!(edge.x, 1440.0 + 2560.0 - 320.0);
    }

    #[test]
    fn popover_opens_below_a_menu_bar_on_a_monitor_underneath() {
        let monitor = Area {
            x: 0.0,
            y: 900.0,
            width: 1440.0,
            height: 900.0,
        };
        let origin = popover_origin(
            &icon_rect(700.0, 900.0, 120.0, 22.0),
            1.0,
            320.0,
            240.0,
            monitor,
        );
        assert_eq!(origin.x, 600.0);
        assert_eq!(origin.y, 928.0);
    }

    #[test]
    fn blur_guard_only_covers_the_first_400ms() {
        let now = Instant::now();
        assert!(!blur_guard_active(None, now));
        assert!(blur_guard_active(
            Some(now - Duration::from_millis(100)),
            now
        ));
        assert!(!blur_guard_active(
            Some(now - Duration::from_millis(600)),
            now
        ));
    }
}
