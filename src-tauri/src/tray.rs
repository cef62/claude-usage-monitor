//! Tray icon, its title text, the right-click menu, and popover placement.

use crate::poll::{self, Snapshot, Status};
use crate::settings::{self, Settings, KEYS};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, LogicalPosition, Manager, Rect, Wry};

pub const POPOVER_WIDTH: f64 = 320.0;
pub const TRAY_ID: &str = "main";
const USAGE_URL: &str = "https://claude.ai/settings/usage";
const POPOVER_GAP: f64 = 6.0;
pub const SESSION_GLYPH: &str = "◷";
pub const WEEKLY_GLYPH: &str = "▦";
pub const SEPARATOR: &str = "  ·  ";

/// Check items of the "Menu bar" submenu, kept so the handler can re-sync check marks.
pub struct MenuItems(pub HashMap<String, CheckMenuItem<Wry>>);

const LABELS: [(&str, &str); 5] = [
    ("session", "Session"),
    ("weekly", "Weekly"),
    ("glyph", "Glyphs"),
    ("percent", "Percent"),
    ("remaining", "Remaining time"),
];

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
        format!(" {}", halves.join(SEPARATOR))
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
    let current = *lock_settings(app);
    let open = MenuItemBuilder::with_id("open", "Open usage page").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let mut items = HashMap::new();
    let mut submenu = SubmenuBuilder::new(app, "Menu bar");
    for (key, label) in LABELS {
        let item = CheckMenuItem::with_id(
            app,
            format!("set:{key}"),
            label,
            true,
            current.get(key),
            None::<&str>,
        )?;
        submenu = submenu.item(&item);
        items.insert(key.to_string(), item);
    }
    let submenu = submenu.build()?;
    app.manage(MenuItems(items));

    let menu = MenuBuilder::new(app)
        .item(&open)
        .item(&submenu)
        .separator()
        .item(&quit)
        .build()?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .title(title(&Snapshot::default(), poll::now(), &current))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "open" => {
                    let _ = tauri_plugin_opener::open_url(USAGE_URL, None::<&str>);
                }
                "quit" => app.exit(0),
                _ => {
                    if let Some(key) = id.strip_prefix("set:") {
                        on_setting_toggled(app, key);
                    }
                }
            }
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
    app.state::<Mutex<Settings>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn on_setting_toggled(app: &AppHandle, key: &str) {
    let updated = {
        let mut s = lock_settings(app);
        s.toggle(key);
        *s
    };
    // Re-sync every check mark so a refused toggle snaps back.
    if let Some(items) = app.try_state::<MenuItems>() {
        for k in KEYS {
            if let Some(item) = items.0.get(k) {
                let _ = item.set_checked(updated.get(k));
            }
        }
    }
    if let Some(path) = settings::path(app) {
        // The in-memory value already applies; a failed write only loses persistence.
        let _ = settings::save(&path, &updated);
    }
    refresh_title(app, &poll::read(&app.state::<poll::Shared>()));
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
    let settings = *lock_settings(app);
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
