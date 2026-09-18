//! Windows tray icon: two mini bars (session, weekly) rendered into raw RGBA.
//! Pure so it is unit-testable pixel by pixel; macOS keeps the template glyph and title.

use crate::alerts;
use crate::poll::{Snapshot, Status};
use crate::settings::Settings;
use crate::usage::Quota;

/// Rendered size; Windows scales it to 16×16 at 100 % DPI.
pub const SIZE: u32 = 32;
pub const OK: [u8; 4] = [0x34, 0xC7, 0x59, 0xFF];
pub const WARN: [u8; 4] = [0xF5, 0xA6, 0x23, 0xFF];
pub const OVER: [u8; 4] = [0xE5, 0x48, 0x3C, 0xFF];
/// Translucent grey so the empty part of a bar reads on both dark and light taskbars.
pub const TRACK: [u8; 4] = [0x80, 0x80, 0x80, 0x90];
/// Fill colour while the numbers are not live (no token, expired, error).
pub const IDLE: [u8; 4] = [0xA0, 0xA0, 0xA0, 0xFF];

const BAR_X: u32 = 3;
const BAR_W: u32 = 26;
const BAR_H: u32 = 8;
const TOP_Y: u32 = 6;
const BOTTOM_Y: u32 = 18;
const SINGLE_Y: u32 = 12;
const SQUARE_XY: u32 = 13;
const SQUARE_SIZE: u32 = 6;
const BORDER: u32 = 2;

/// Same rule as `barColor` in `src/lib/format.ts`, so the icon and the popover never disagree.
pub fn bar_color(percent: f64, elapsed_pct: f64) -> [u8; 4] {
    if percent >= 100.0 || percent > elapsed_pct {
        OVER
    } else if percent >= 80.0 {
        WARN
    } else {
        OK
    }
}

struct Canvas(Vec<u8>);

impl Canvas {
    fn new() -> Self {
        Self(vec![0; (SIZE * SIZE * 4) as usize])
    }

    fn fill(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
        // Every caller passes fixed constants; a bad constant should fail a test, not a user.
        debug_assert!(x + w <= SIZE && y + h <= SIZE, "fill outside the canvas");
        for yy in y..y + h {
            for xx in x..x + w {
                let i = ((yy * SIZE + xx) * 4) as usize;
                self.0[i..i + 4].copy_from_slice(&color);
            }
        }
    }
}

fn bar(canvas: &mut Canvas, y: u32, quota: Option<&Quota>, now: i64, live: bool) {
    canvas.fill(BAR_X, y, BAR_W, BAR_H, TRACK);
    let Some(q) = quota else {
        return;
    };
    let width = (f64::from(BAR_W) * q.percent.clamp(0.0, 100.0) / 100.0).round() as u32;
    let color = if live {
        bar_color(q.percent, alerts::elapsed_pct(q, now))
    } else {
        IDLE
    };
    canvas.fill(BAR_X, y, width, BAR_H, color);
}

/// 32×32 RGBA, row-major. Bars follow the same `session`/`weekly` display settings as the title.
pub fn render(s: &Snapshot, settings: &Settings, now: i64) -> Vec<u8> {
    let mut canvas = Canvas::new();
    let live = matches!(s.status, Status::Ok | Status::RateLimited { .. });
    let find = |key: &str| s.quotas.iter().find(|q| q.key == key);
    // Both halves off is impossible: `Settings::toggle` keeps at least one on.
    match (settings.session, settings.weekly) {
        (true, true) => {
            bar(&mut canvas, TOP_Y, find("session"), now, live);
            bar(&mut canvas, BOTTOM_Y, find("weekly"), now, live);
        }
        (true, false) => bar(&mut canvas, SINGLE_Y, find("session"), now, live),
        (false, _) => bar(&mut canvas, SINGLE_Y, find("weekly"), now, live),
    }
    if matches!(s.status, Status::NoToken | Status::AuthExpired) {
        canvas.fill(SQUARE_XY, SQUARE_XY, SQUARE_SIZE, SQUARE_SIZE, OVER);
    }
    if alerts::marker(&s.quotas, settings) {
        canvas.fill(0, 0, SIZE, BORDER, OVER);
        canvas.fill(0, SIZE - BORDER, SIZE, BORDER, OVER);
        canvas.fill(0, 0, BORDER, SIZE, OVER);
        canvas.fill(SIZE - BORDER, 0, BORDER, SIZE, OVER);
    }
    canvas.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poll::{Snapshot, Status};
    use crate::settings::Settings;
    use crate::usage::{Quota, SESSION_SECS, WEEKLY_SECS};

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

    // Session 48.4 % with 2 h 13 m left (elapsed 56 %), weekly 40 % with 3 d left (elapsed 57 %):
    // both behind the clock, so both bars are green.
    fn behind_clock() -> Vec<Quota> {
        vec![
            quota("session", 48.4, 2 * 3600 + 13 * 60, SESSION_SECS),
            quota("weekly", 40.0, 3 * 86400, WEEKLY_SECS),
            quota("weekly:fable", 99.0, 3 * 86400, WEEKLY_SECS),
        ]
    }

    fn snapshot(status: Status, quotas: Vec<Quota>) -> Snapshot {
        Snapshot {
            quotas,
            fetched_at: Some(NOW),
            next_poll_at: NOW + 180,
            status,
        }
    }

    fn px(buf: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn bar_color_follows_the_popover_rule() {
        assert_eq!(bar_color(48.0, 60.0), OK);
        assert_eq!(bar_color(85.0, 60.0), OVER);
        assert_eq!(bar_color(85.0, 90.0), WARN);
        assert_eq!(bar_color(100.0, 100.0), OVER);
    }

    #[test]
    fn render_has_the_right_length() {
        let buf = render(&Snapshot::default(), &Settings::default(), NOW);
        assert_eq!(buf.len(), (SIZE * SIZE * 4) as usize);
    }

    #[test]
    fn two_bars_fill_by_percent_and_ignore_scoped_quotas() {
        let buf = render(
            &snapshot(Status::Ok, behind_clock()),
            &Settings::default(),
            NOW,
        );
        // session: round(26 × 0.484) = 13 px → x 3..=15
        assert_eq!(px(&buf, 3, 8), OK);
        assert_eq!(px(&buf, 15, 8), OK);
        assert_eq!(px(&buf, 16, 8), TRACK);
        assert_eq!(px(&buf, 28, 8), TRACK);
        // weekly: round(26 × 0.40) = 10 px → x 3..=12
        assert_eq!(px(&buf, 12, 20), OK);
        assert_eq!(px(&buf, 13, 20), TRACK);
        // gap between bars and the corners stay transparent
        assert_eq!(px(&buf, 16, 15), [0, 0, 0, 0]);
        assert_eq!(px(&buf, 0, 0), [0, 0, 0, 0]);
        assert_eq!(px(&buf, 2, 8), [0, 0, 0, 0]);
        assert_eq!(px(&buf, 29, 8), [0, 0, 0, 0]);
    }

    #[test]
    fn hidden_weekly_centres_the_session_bar() {
        let mut settings = Settings::default();
        assert!(settings.toggle("weekly"));
        let buf = render(&snapshot(Status::Ok, behind_clock()), &settings, NOW);
        assert_eq!(px(&buf, 3, 8), [0, 0, 0, 0]);
        assert_eq!(px(&buf, 3, 20), [0, 0, 0, 0]);
        assert_eq!(px(&buf, 3, 15), OK);
        assert_eq!(px(&buf, 15, 12), OK);
        assert_eq!(px(&buf, 16, 19), TRACK);
    }

    #[test]
    fn hidden_session_centres_the_weekly_bar() {
        let mut settings = Settings::default();
        assert!(settings.toggle("session"));
        let buf = render(&snapshot(Status::Ok, behind_clock()), &settings, NOW);
        assert_eq!(px(&buf, 12, 15), OK);
        assert_eq!(px(&buf, 13, 15), TRACK);
        assert_eq!(px(&buf, 3, 8), [0, 0, 0, 0]);
    }

    #[test]
    fn missing_quota_draws_the_track_only() {
        let only_session = vec![quota("session", 48.4, 2 * 3600, SESSION_SECS)];
        let buf = render(
            &snapshot(Status::Ok, only_session),
            &Settings::default(),
            NOW,
        );
        assert_eq!(px(&buf, 3, 20), TRACK);
        assert_eq!(px(&buf, 28, 20), TRACK);
    }

    #[test]
    fn ahead_of_clock_is_red_and_over_80_is_amber() {
        let quotas = vec![
            quota("session", 85.0, 4 * 3600, SESSION_SECS), // elapsed 20 % → ahead → red
            quota("weekly", 85.0, 12 * 3600, WEEKLY_SECS),  // elapsed 93 % → behind, ≥ 80 → amber
        ];
        let buf = render(&snapshot(Status::Ok, quotas), &Settings::default(), NOW);
        assert_eq!(px(&buf, 3, 8), OVER);
        assert_eq!(px(&buf, 3, 20), WARN);
    }

    #[test]
    fn alert_marker_draws_a_red_border() {
        let quotas = vec![quota("session", 96.0, 3600, SESSION_SECS)];
        let buf = render(&snapshot(Status::Ok, quotas), &Settings::default(), NOW);
        assert_eq!(px(&buf, 0, 0), OVER);
        assert_eq!(px(&buf, 31, 31), OVER);
        assert_eq!(px(&buf, 1, 16), OVER);
        assert_eq!(px(&buf, 16, 30), OVER);
        assert_eq!(px(&buf, 2, 2), [0, 0, 0, 0]);
    }

    #[test]
    fn no_marker_when_alerts_are_off() {
        let quotas = vec![quota("session", 96.0, 3600, SESSION_SECS)];
        let mut settings = Settings::default();
        assert!(settings.toggle("alert_session"));
        let buf = render(&snapshot(Status::Ok, quotas), &settings, NOW);
        assert_eq!(px(&buf, 0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn no_token_greys_fills_and_draws_the_square() {
        let buf = render(
            &snapshot(Status::NoToken, behind_clock()),
            &Settings::default(),
            NOW,
        );
        assert_eq!(px(&buf, 3, 8), IDLE);
        assert_eq!(px(&buf, 3, 20), IDLE);
        assert_eq!(px(&buf, 16, 16), OVER);
        assert_eq!(px(&buf, 13, 13), OVER);
        assert_eq!(px(&buf, 18, 18), OVER);
        assert_eq!(px(&buf, 12, 16), [0, 0, 0, 0]);
    }

    #[test]
    fn auth_expired_draws_the_square_and_error_only_greys() {
        let expired = render(
            &snapshot(Status::AuthExpired, behind_clock()),
            &Settings::default(),
            NOW,
        );
        assert_eq!(px(&expired, 16, 16), OVER);
        let error = render(
            &snapshot(
                Status::Error {
                    message: "boom".into(),
                },
                behind_clock(),
            ),
            &Settings::default(),
            NOW,
        );
        assert_eq!(px(&error, 3, 8), IDLE);
        assert_eq!(px(&error, 16, 16), [0, 0, 0, 0]);
    }

    #[test]
    fn rate_limited_keeps_live_colours() {
        let buf = render(
            &snapshot(Status::RateLimited { until: NOW + 300 }, behind_clock()),
            &Settings::default(),
            NOW,
        );
        assert_eq!(px(&buf, 3, 8), OK);
    }

    #[test]
    fn percent_over_100_clamps_the_fill() {
        let quotas = vec![quota("session", 130.0, 3600, SESSION_SECS)];
        let buf = render(&snapshot(Status::Ok, quotas), &Settings::default(), NOW);
        assert_eq!(px(&buf, 28, 8), OVER);
        assert_eq!(px(&buf, 29, 8), [0, 0, 0, 0]);
    }
}
