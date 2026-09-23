//! Threshold alerts: pure decision logic. Delivery (notifications) lives in main.rs.

use crate::forecast::{self, Forecast};
use crate::settings::Settings;
use crate::usage::Quota;
use std::collections::{HashMap, HashSet};

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

/// Levels already announced, keyed by (quota key, reset window, level), plus the window in
/// which each quota last hit its top level — that arms the reset notification — and the
/// windows that already had a run-out notification. In-memory only.
#[derive(Default)]
pub struct AlertState {
    fired: HashSet<(String, i64, u8)>,
    armed: HashMap<String, i64>,
    forecast_fired: HashSet<(String, i64)>,
}

/// A quota whose window rolled over after the top level had fired in the previous one.
#[derive(Debug, Clone, PartialEq)]
pub struct Reset {
    pub key: String,
    pub label: String,
    pub resets_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub key: String,
    pub label: String,
    pub level: u8,
    pub percent: f64,
    pub resets_at: i64,
}

/// Quotas that reached their top level and have since rolled into a new window. Each fires once;
/// the arming survives toggles being off so it is delivered when they come back on.
pub fn resets(state: &mut AlertState, quotas: &[Quota], settings: &Settings) -> Vec<Reset> {
    if !settings.alert_reset {
        return Vec::new();
    }
    let mut out = Vec::new();
    for q in quotas.iter().filter(|q| enabled(settings, &q.key)) {
        let rolled = state
            .armed
            .get(&q.key)
            .is_some_and(|armed_at| q.resets_at - armed_at > SAME_WINDOW_SECS);
        if rolled {
            state.armed.remove(&q.key);
            out.push(Reset {
                key: q.key.clone(),
                label: label(&q.key).to_string(),
                resets_at: q.resets_at,
            });
        }
    }
    out
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

/// The highest configured level: `⚠` shows at or above it and it fires regardless of the clock.
pub fn top(levels: &[u8]) -> Option<u8> {
    levels.iter().copied().max()
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
        let levels = settings.levels(&q.key);
        let Some(top_level) = top(levels) else {
            continue;
        };
        let elapsed = elapsed_pct(q, now);
        let highest = levels
            .iter()
            .copied()
            .filter(|&level| q.percent >= f64::from(level))
            .filter(|&level| level >= top_level || q.percent > elapsed)
            .filter(|&level| !already_fired(state, &q.key, q.resets_at, level))
            .max();
        let Some(highest) = highest else {
            continue;
        };
        for &level in levels.iter().filter(|&&l| l <= highest) {
            state.fired.insert((q.key.clone(), q.resets_at, level));
        }
        if highest == top_level {
            state.armed.insert(q.key.clone(), q.resets_at);
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

/// True when any alert-enabled quota is at or above the marker level (the highest configured
/// level, where time-aware suppression no longer applies).
pub fn marker(quotas: &[Quota], settings: &Settings) -> bool {
    quotas.iter().any(|q| {
        enabled(settings, &q.key)
            && top(settings.levels(&q.key)).is_some_and(|t| q.percent >= f64::from(t))
    })
}

/// A quota the pace forecast says hits 100 % well before its reset.
#[derive(Debug, Clone, PartialEq)]
pub struct RunOut {
    pub key: String,
    pub label: String,
    pub at: i64,
    pub resets_at: i64,
}

/// At most one per quota per window, only when running out costs at least one lookback of the
/// window, and only below the top level, where the threshold alert already speaks. Returned
/// entries count as fired whether the caller sends them alone or merges them into an alert.
pub fn run_outs(
    state: &mut AlertState,
    quotas: &[Quota],
    forecasts: &HashMap<String, Forecast>,
    settings: &Settings,
    now: i64,
) -> Vec<RunOut> {
    state
        .forecast_fired
        .retain(|(_, resets_at)| *resets_at >= now - PRUNE_AFTER_SECS);
    if !settings.alert_forecast {
        return Vec::new();
    }
    let mut out = Vec::new();
    for q in quotas.iter().filter(|q| enabled(settings, &q.key)) {
        let Some(Forecast::RunsOut {
            at,
            from_average: false,
        }) = forecasts.get(&q.key)
        else {
            continue;
        };
        let early_enough = q.resets_at - at >= forecast::lookback(q.period_secs);
        let below_top = top(settings.levels(&q.key)).is_none_or(|t| q.percent < f64::from(t));
        let fired = state
            .forecast_fired
            .iter()
            .any(|(k, r)| *k == q.key && (r - q.resets_at).abs() <= SAME_WINDOW_SECS);
        if !early_enough || !below_top || fired {
            continue;
        }
        state.forecast_fired.insert((q.key.clone(), q.resets_at));
        out.push(RunOut {
            key: q.key.clone(),
            label: label(&q.key).to_string(),
            at: *at,
            resets_at: q.resets_at,
        });
    }
    out
}

/// Splits run-outs into those that ride along with a threshold alert for the same quota in
/// this cycle (one notification instead of two) and those sent on their own.
pub fn partition_run_outs(due: &[Alert], run_outs: Vec<RunOut>) -> (Vec<RunOut>, Vec<RunOut>) {
    run_outs
        .into_iter()
        .partition(|r| due.iter().any(|a| a.key == r.key))
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

    fn with_levels(session: &[u8], weekly: &[u8]) -> Settings {
        let mut s = Settings::default();
        assert!(s.set_levels("session", session));
        assert!(s.set_levels("weekly", weekly));
        s
    }

    #[test]
    fn custom_levels_top_is_unconditional_and_lower_is_time_aware() {
        let s = with_levels(&[90, 95], &[95]);
        let mut st = AlertState::default();
        // 92% with 20 min left (elapsed ~93%): 92 is behind the clock → silent.
        assert!(evaluate(&mut st, &[session(92.0, 1200)], &s, NOW).is_empty());
        // 96%: top level fires regardless of the clock.
        let a = evaluate(&mut st, &[session(96.0, 1800)], &s, NOW);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].level, 95);
    }

    #[test]
    fn single_level_is_the_top() {
        let s = with_levels(&[95], &[95]);
        let mut st = AlertState::default();
        assert!(evaluate(&mut st, &[session(85.0, 7200)], &s, NOW).is_empty());
        assert_eq!(evaluate(&mut st, &[session(95.0, 60)], &s, NOW).len(), 1);
    }

    #[test]
    fn three_levels_fire_the_lowest_first() {
        let s = with_levels(&[50, 80, 95], &[95]);
        let mut st = AlertState::default();
        let a = evaluate(&mut st, &[session(55.0, 3 * 3600)], &s, NOW);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].level, 50);
    }

    #[test]
    fn marker_uses_top_level() {
        let s = with_levels(&[90, 95], &[95]);
        assert!(!marker(&[session(92.0, 7200)], &s));
        assert!(marker(&[session(96.0, 7200)], &s));
        assert_eq!(top(&[90, 95]), Some(95));
        assert_eq!(top(&[]), None);
    }

    fn next_window(q: &Quota) -> Quota {
        Quota {
            resets_at: q.resets_at + q.period_secs as i64,
            ..q.clone()
        }
    }

    #[test]
    fn reset_fires_once_after_the_top_level_was_reached() {
        let mut st = AlertState::default();
        let blocked = session(96.0, 3600);
        assert_eq!(evaluate(&mut st, &[blocked.clone()], &on(), NOW).len(), 1);
        assert!(resets(&mut st, &[blocked.clone()], &on()).is_empty());
        let fresh = next_window(&blocked);
        let out = resets(&mut st, &[fresh.clone()], &on());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "session");
        assert_eq!(out[0].label, "Session");
        assert_eq!(out[0].resets_at, fresh.resets_at);
        assert!(resets(&mut st, &[fresh], &on()).is_empty());
    }

    #[test]
    fn reset_needs_the_top_level_not_just_any_alert() {
        let mut st = AlertState::default();
        let warm = session(85.0, 7200); // fires 80, not 95
        assert_eq!(evaluate(&mut st, &[warm.clone()], &on(), NOW).len(), 1);
        assert!(resets(&mut st, &[next_window(&warm)], &on()).is_empty());
    }

    #[test]
    fn reset_ignores_jitter_and_respects_toggles() {
        let mut st = AlertState::default();
        let blocked = session(96.0, 3600);
        evaluate(&mut st, &[blocked.clone()], &on(), NOW);
        let jittered = Quota {
            resets_at: blocked.resets_at + 1,
            ..blocked.clone()
        };
        assert!(resets(&mut st, &[jittered], &on()).is_empty());

        let mut off = on();
        assert!(off.toggle("alert_reset"));
        assert!(resets(&mut st, &[next_window(&blocked)], &off).is_empty());

        let mut no_session = on();
        assert!(no_session.toggle("alert_session"));
        assert!(resets(&mut st, &[next_window(&blocked)], &no_session).is_empty());
        // still armed: turning the toggles back on delivers it
        assert_eq!(resets(&mut st, &[next_window(&blocked)], &on()).len(), 1);
    }

    fn runs_out(key: &str, at: i64) -> HashMap<String, Forecast> {
        HashMap::from([(
            key.to_string(),
            Forecast::RunsOut {
                at,
                from_average: false,
            },
        )])
    }

    #[test]
    fn run_out_fires_once_per_window() {
        let mut st = AlertState::default();
        let q = session(40.0, 4 * 3600);
        let f = runs_out("session", NOW + 3600);
        let out = run_outs(&mut st, &[q.clone()], &f, &on(), NOW);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "session");
        assert_eq!(out[0].label, "Session");
        assert_eq!(out[0].at, NOW + 3600);
        assert_eq!(out[0].resets_at, q.resets_at);
        assert!(run_outs(&mut st, &[q.clone()], &f, &on(), NOW + 180).is_empty());
        let jittered = Quota {
            resets_at: q.resets_at + 1,
            ..q.clone()
        };
        assert!(run_outs(&mut st, &[jittered], &f, &on(), NOW + 360).is_empty());
        let next = next_window(&q);
        let later = runs_out("session", next.resets_at - 3 * 3600);
        assert_eq!(
            run_outs(&mut st, &[next], &later, &on(), NOW + 4 * 3600).len(),
            1
        );
    }

    #[test]
    fn a_window_average_run_out_never_alerts_nor_uses_up_the_window() {
        // Early in a window the base is the window start: a burst then quiet looks like a run-out.
        let mut st = AlertState::default();
        let q = session(40.0, 4 * 3600);
        let average = HashMap::from([(
            "session".to_string(),
            Forecast::RunsOut {
                at: NOW + 3600,
                from_average: true,
            },
        )]);
        assert!(run_outs(&mut st, &[q.clone()], &average, &on(), NOW).is_empty());
        let trend = runs_out("session", NOW + 3600);
        assert_eq!(run_outs(&mut st, &[q], &trend, &on(), NOW + 180).len(), 1);
    }

    #[test]
    fn run_out_needs_both_toggles() {
        let q = session(40.0, 4 * 3600);
        let f = runs_out("session", NOW + 3600);
        let mut off = on();
        assert!(off.toggle("alert_forecast"));
        assert!(run_outs(&mut AlertState::default(), &[q.clone()], &f, &off, NOW).is_empty());
        let mut no_session = on();
        assert!(no_session.toggle("alert_session"));
        assert!(run_outs(&mut AlertState::default(), &[q], &f, &no_session, NOW).is_empty());
    }

    #[test]
    fn run_out_close_to_the_reset_is_silent() {
        // Resets in 4 h; running out 30 min before is less than the ~43 min lookback.
        let q = session(40.0, 4 * 3600);
        let f = runs_out("session", q.resets_at - 1800);
        assert!(run_outs(&mut AlertState::default(), &[q], &f, &on(), NOW).is_empty());
    }

    #[test]
    fn run_out_is_silent_at_the_top_level() {
        let q = session(95.0, 4 * 3600);
        let f = runs_out("session", NOW + 600);
        assert!(run_outs(&mut AlertState::default(), &[q], &f, &on(), NOW).is_empty());
    }

    #[test]
    fn at_reset_forecasts_never_fire() {
        let q = session(40.0, 4 * 3600);
        let f = HashMap::from([("session".to_string(), Forecast::AtReset { percent: 70.0 })]);
        assert!(run_outs(&mut AlertState::default(), &[q], &f, &on(), NOW).is_empty());
    }

    #[test]
    fn run_out_entries_from_old_windows_are_pruned() {
        let mut st = AlertState::default();
        st.forecast_fired
            .insert(("session".to_string(), NOW - 2 * 86400));
        run_outs(&mut st, &[], &HashMap::new(), &on(), NOW);
        assert!(st.forecast_fired.is_empty());
    }

    #[test]
    fn run_outs_merge_only_into_a_same_quota_alert() {
        let alert = Alert {
            key: "session".into(),
            label: "Session".into(),
            level: 80,
            percent: 85.0,
            resets_at: NOW + 7200,
        };
        let r = |key: &str| RunOut {
            key: key.into(),
            label: key.into(),
            at: NOW + 3600,
            resets_at: NOW + 7200,
        };
        let (merged, alone) = partition_run_outs(&[alert], vec![r("session"), r("weekly")]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].key, "session");
        assert_eq!(alone.len(), 1);
        assert_eq!(alone[0].key, "weekly");
    }
}
