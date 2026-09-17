//! Threshold alerts: pure decision logic. Delivery (notifications) lives in main.rs.

use crate::settings::Settings;
use crate::usage::Quota;
use std::collections::HashSet;

pub const SESSION_LEVELS: [u8; 2] = [80, 95];
pub const WEEKLY_LEVELS: [u8; 1] = [95];
/// At or above this level the title shows `⚠` and time-aware suppression no longer applies.
pub const MARKER_LEVEL: u8 = 95;
const PRUNE_AFTER_SECS: i64 = 86_400;
/// `resets_at` jitters sub-second between polls (docs/research-usage-monitors.md); anything
/// within a minute is the same reset window.
const SAME_WINDOW_SECS: i64 = 60;

fn already_fired(state: &AlertState, key: &str, resets_at: i64, level: u8) -> bool {
    state
        .fired
        .iter()
        .any(|(k, r, l)| k == key && *l == level && (r - resets_at).abs() <= SAME_WINDOW_SECS)
}

/// Levels already announced, keyed by (quota key, reset window, level). In-memory only.
#[derive(Default)]
pub struct AlertState {
    fired: HashSet<(String, i64, u8)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub key: String,
    pub label: String,
    pub level: u8,
    pub percent: f64,
    pub resets_at: i64,
}

/// Fraction of the reset window already elapsed, 0..=100.
pub fn elapsed_pct(q: &Quota, now: i64) -> f64 {
    let period = q.period_secs as f64;
    if period <= 0.0 {
        return 100.0;
    }
    let elapsed = period - (q.resets_at - now) as f64;
    (elapsed / period * 100.0).clamp(0.0, 100.0)
}

pub fn enabled(settings: &Settings, key: &str) -> bool {
    match key {
        "session" => settings.alert_session,
        "weekly" => settings.alert_weekly,
        _ => false,
    }
}

fn levels(key: &str) -> &'static [u8] {
    match key {
        "session" => &SESSION_LEVELS,
        "weekly" => &WEEKLY_LEVELS,
        _ => &[],
    }
}

fn label(key: &str) -> &'static str {
    match key {
        "session" => "Session",
        "weekly" => "Weekly",
        _ => "Quota",
    }
}

/// Returns at most one alert per enabled quota: the highest level that is newly due. Lower
/// levels are recorded as fired at the same time so they never announce late.
pub fn evaluate(
    state: &mut AlertState,
    quotas: &[Quota],
    settings: &Settings,
    now: i64,
) -> Vec<Alert> {
    state
        .fired
        .retain(|(_, resets_at, _)| *resets_at >= now - PRUNE_AFTER_SECS);

    let mut alerts = Vec::new();
    for q in quotas.iter().filter(|q| enabled(settings, &q.key)) {
        let elapsed = elapsed_pct(q, now);
        let highest = levels(&q.key)
            .iter()
            .copied()
            .filter(|&level| q.percent >= f64::from(level))
            .filter(|&level| level >= MARKER_LEVEL || q.percent > elapsed)
            .filter(|&level| !already_fired(state, &q.key, q.resets_at, level))
            .max();
        let Some(highest) = highest else {
            continue;
        };
        for &level in levels(&q.key).iter().filter(|&&l| l <= highest) {
            state.fired.insert((q.key.clone(), q.resets_at, level));
        }
        alerts.push(Alert {
            key: q.key.clone(),
            label: label(&q.key).to_string(),
            level: highest,
            percent: q.percent,
            resets_at: q.resets_at,
        });
    }
    alerts
}

/// True when any alert-enabled quota is at or above the marker level.
pub fn marker(quotas: &[Quota], settings: &Settings) -> bool {
    quotas
        .iter()
        .any(|q| enabled(settings, &q.key) && q.percent >= f64::from(MARKER_LEVEL))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::usage::{Quota, SESSION_SECS, WEEKLY_SECS};

    const NOW: i64 = 1_789_588_800;

    fn quota(key: &str, percent: f64, secs_left: i64, period: u64) -> Quota {
        Quota {
            key: key.to_string(),
            label: key.to_string(),
            percent,
            resets_at: NOW + secs_left,
            period_secs: period,
        }
    }

    fn session(percent: f64, secs_left: i64) -> Quota {
        quota("session", percent, secs_left, SESSION_SECS)
    }

    fn on() -> Settings {
        Settings::default()
    }

    #[test]
    fn elapsed_pct_tracks_the_window() {
        assert!((elapsed_pct(&session(0.0, 7200), NOW) - 60.0).abs() < 0.01);
        assert_eq!(
            elapsed_pct(&session(0.0, 2 * SESSION_SECS as i64), NOW),
            0.0
        );
        assert_eq!(elapsed_pct(&session(0.0, -10), NOW), 100.0);
    }

    #[test]
    fn fires_80_when_ahead_of_clock() {
        let mut st = AlertState::default();
        let alerts = evaluate(&mut st, &[session(85.0, 7200)], &on(), NOW);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, 80);
        assert_eq!(alerts[0].label, "Session");
        assert_eq!(alerts[0].percent, 85.0);
    }

    #[test]
    fn silent_below_every_level() {
        let mut st = AlertState::default();
        assert!(evaluate(&mut st, &[session(30.0, 4 * 3600)], &on(), NOW).is_empty());
    }

    #[test]
    fn time_aware_suppresses_80_behind_the_clock() {
        let mut st = AlertState::default();
        // 30 minutes left → 90% elapsed; 82% used is behind the clock.
        assert!(evaluate(&mut st, &[session(82.0, 1800)], &on(), NOW).is_empty());
    }

    #[test]
    fn fires_95_once_regardless_of_clock() {
        let mut st = AlertState::default();
        let first = evaluate(&mut st, &[session(96.0, 1800)], &on(), NOW);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].level, 95);
        // Same window (same resets_at), later poll.
        assert!(evaluate(&mut st, &[session(97.0, 1800)], &on(), NOW + 100).is_empty());
    }

    #[test]
    fn highest_due_level_marks_lower_levels_fired() {
        let mut st = AlertState::default();
        let first = evaluate(&mut st, &[session(96.0, 7200)], &on(), NOW);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].level, 95);
        // Usage drops back into the 80 band inside the same window: nothing new.
        assert!(evaluate(&mut st, &[session(85.0, 7200)], &on(), NOW + 200).is_empty());
    }

    #[test]
    fn new_window_rearms() {
        let mut st = AlertState::default();
        assert_eq!(
            evaluate(&mut st, &[session(85.0, 7200)], &on(), NOW).len(),
            1
        );
        let later = NOW + 8000;
        let next = Quota {
            resets_at: later + 7200,
            ..session(85.0, 0)
        };
        assert_eq!(evaluate(&mut st, &[next], &on(), later).len(), 1);
    }

    #[test]
    fn disabled_quota_is_silent() {
        let mut st = AlertState::default();
        let mut s = on();
        assert!(s.toggle("alert_session"));
        assert!(evaluate(&mut st, &[session(96.0, 7200)], &s, NOW).is_empty());
    }

    #[test]
    fn weekly_uses_its_own_levels() {
        let mut st = AlertState::default();
        let weekly = |p: f64| quota("weekly", p, 3 * 86400, WEEKLY_SECS);
        assert!(evaluate(&mut st, &[weekly(90.0)], &on(), NOW).is_empty());
        let alerts = evaluate(&mut st, &[weekly(96.0)], &on(), NOW);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].level, 95);
        assert_eq!(alerts[0].label, "Weekly");
    }

    #[test]
    fn scoped_quotas_are_ignored() {
        let mut st = AlertState::default();
        let scoped = quota("weekly:fable", 99.0, 3 * 86400, WEEKLY_SECS);
        assert!(evaluate(&mut st, &[scoped], &on(), NOW).is_empty());
    }

    #[test]
    fn prunes_entries_from_old_windows() {
        let mut st = AlertState::default();
        st.fired
            .insert(("session".to_string(), NOW - 2 * 86400, 80));
        evaluate(&mut st, &[], &on(), NOW);
        assert!(st.fired.is_empty());
    }

    #[test]
    fn jittered_resets_at_is_the_same_window() {
        let mut st = AlertState::default();
        assert_eq!(
            evaluate(&mut st, &[session(96.0, 7200)], &on(), NOW).len(),
            1
        );
        let jittered = Quota {
            resets_at: NOW + 7201,
            ..session(97.0, 7200)
        };
        assert!(evaluate(&mut st, &[jittered], &on(), NOW + 100).is_empty());
    }

    #[test]
    fn marker_follows_alert_settings_not_display_settings() {
        let quotas = [
            session(95.0, 7200),
            quota("weekly", 40.0, 86400, WEEKLY_SECS),
        ];
        assert!(marker(&quotas, &on()));
        let mut off = on();
        assert!(off.toggle("alert_session"));
        assert!(!marker(&quotas, &off));
        let mut hidden = on();
        assert!(hidden.toggle("weekly"));
        let weekly_high = [quota("weekly", 96.0, 86400, WEEKLY_SECS)];
        assert!(marker(&weekly_high, &hidden));
    }
}
