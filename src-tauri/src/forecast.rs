//! Pace forecast: where a quota lands at the recent rate. Pure; computed once per successful
//! poll from the full-resolution history samples, shared by the popover and the alerts.

use crate::history::{self, History, Sample};
use crate::usage::Quota;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Forecast {
    /// 100 % is reached by the reset, at this unix second (`at <= resets_at`).
    RunsOut {
        at: i64,
        /// No sample was a full lookback old, so the rate is the window average: fine for the
        /// popover, too jumpy after an early burst to spend the window's only alert on.
        #[serde(skip)]
        from_average: bool,
    },
    /// Projected utilization when the window resets; below 100 up to float rounding (the popover
    /// caps it at 99, alerts only read `RunsOut`).
    AtReset { percent: f64 },
}

/// `period_secs / 7`: ~43 min for the session, exactly one day for the weekly quota, so the
/// weekly rate always spans a full day-night cycle.
pub fn lookback(period_secs: u64) -> i64 {
    period_secs as i64 / 7
}

pub fn forecast(q: &Quota, samples: &[Sample], now: i64) -> Option<Forecast> {
    let lookback = lookback(q.period_secs);
    let window_start = q.resets_at - q.period_secs as i64;
    if q.percent >= 100.0 || now >= q.resets_at || now - window_start < lookback / 2 {
        return None;
    }
    // Windows start at 0 %, so without an old-enough sample this is the window average.
    let base = samples
        .iter()
        .rev()
        .find(|s| s.t >= window_start && s.t <= now - lookback);
    let from_average = base.is_none();
    let (t0, p0) = base.map_or((window_start, 0.0), |s| (s.t, f64::from(s.pct)));
    let dt = (now - t0) as f64;
    if dt <= 0.0 {
        return None;
    }
    let rate = (q.percent - p0) / dt;
    if rate <= 0.0 {
        return Some(Forecast::AtReset { percent: q.percent });
    }
    // Compared as floats before any cast: a microscopic rate (f32 noise) must not overflow i64.
    // `<=`: reaching 100 % exactly at the reset is a run-out, so `AtReset` stays below 100.
    let secs_to_full = (100.0 - q.percent) / rate;
    let secs_to_reset = (q.resets_at - now) as f64;
    if secs_to_full <= secs_to_reset {
        Some(Forecast::RunsOut {
            at: now + secs_to_full.ceil() as i64,
            from_average,
        })
    } else {
        Some(Forecast::AtReset {
            percent: q.percent + rate * secs_to_reset,
        })
    }
}

/// Forecasts for the quotas in this response only: a quota the API no longer reports (no
/// session running) gets none, even while its old samples are still in the history.
pub fn for_quotas(quotas: &[Quota], history: &History, now: i64) -> HashMap<String, Forecast> {
    quotas
        .iter()
        .filter(|q| history::tracked(&q.key))
        .filter_map(|q| {
            let samples = history.by_key.get(&q.key).map_or(&[][..], Vec::as_slice);
            forecast(q, samples, now).map(|f| (q.key.clone(), f))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{SESSION_SECS, WEEKLY_SECS};

    const NOW: i64 = 1_789_588_800;

    /// Session quota whose window started an hour ago: resets_at = NOW + 14400.
    fn session(percent: f64) -> Quota {
        Quota {
            key: "session".into(),
            label: "Session".into(),
            percent,
            resets_at: NOW + 14_400,
            period_secs: SESSION_SECS,
        }
    }

    fn at(t: i64, pct: f32) -> Sample {
        Sample { t, pct }
    }

    fn runs_out_at(f: Option<Forecast>) -> i64 {
        match f {
            Some(Forecast::RunsOut { at, .. }) => at,
            other => panic!("expected RunsOut, got {other:?}"),
        }
    }

    fn at_reset(f: Option<Forecast>) -> f64 {
        match f {
            Some(Forecast::AtReset { percent }) => percent,
            other => panic!("expected AtReset, got {other:?}"),
        }
    }

    #[test]
    fn lookback_is_a_seventh_of_the_period() {
        assert_eq!(lookback(SESSION_SECS), 2571);
        assert_eq!(lookback(WEEKLY_SECS), 86_400);
    }

    #[test]
    fn steady_rate_runs_out_before_the_reset() {
        let f = forecast(&session(50.0), &[at(NOW - 3000, 20.0), at(NOW, 50.0)], NOW);
        assert!((runs_out_at(f) - (NOW + 5000)).abs() <= 1);
    }

    #[test]
    fn slow_rate_projects_the_percent_at_reset() {
        let f = forecast(&session(30.0), &[at(NOW - 3000, 20.0), at(NOW, 30.0)], NOW);
        assert!((at_reset(f) - 78.0).abs() < 0.01);
    }

    #[test]
    fn reaching_100_exactly_at_the_reset_is_a_run_out() {
        // Binary-exact numbers: 32 % in 4096 s, 64 % left → 100 % in exactly 8192 s = the reset.
        let q = Quota {
            resets_at: NOW + 8192,
            ..session(36.0)
        };
        let f = forecast(&q, &[at(NOW - 4096, 4.0), at(NOW, 36.0)], NOW);
        assert_eq!(runs_out_at(f), NOW + 8192);
    }

    #[test]
    fn flat_usage_stays_where_it_is() {
        let f = forecast(&session(30.0), &[at(NOW - 3000, 30.0), at(NOW, 30.0)], NOW);
        assert!((at_reset(f) - 30.0).abs() < 0.01);
    }

    #[test]
    fn falling_usage_stays_at_the_current_percent() {
        let f = forecast(&session(50.0), &[at(NOW - 3000, 60.0), at(NOW, 50.0)], NOW);
        assert!((at_reset(f) - 50.0).abs() < 0.01);
    }

    #[test]
    fn f32_rounding_noise_is_not_a_run_out() {
        // 48.3 stored as f32 reads back a hair below the fresh f64 48.3: a microscopic rate.
        let f = forecast(&session(48.3), &[at(NOW - 3000, 48.3), at(NOW, 48.3)], NOW);
        assert!((at_reset(f) - 48.3).abs() < 0.01);
    }

    #[test]
    fn base_is_the_newest_sample_at_least_one_lookback_old() {
        // NOW − 1000 is inside the lookback and must be ignored; NOW − 3000 is the base.
        let samples = [
            at(NOW - 3500, 0.0),
            at(NOW - 3000, 20.0),
            at(NOW - 1000, 49.0),
            at(NOW, 50.0),
        ];
        let f = forecast(&session(50.0), &samples, NOW);
        assert!((runs_out_at(f) - (NOW + 5000)).abs() <= 1);
    }

    #[test]
    fn without_an_old_sample_the_window_start_is_the_base() {
        // Window started 3600 s ago at 0 %: 50 % in an hour → 100 % in another hour.
        let f = forecast(&session(50.0), &[at(NOW - 600, 45.0), at(NOW, 50.0)], NOW);
        assert!((runs_out_at(f) - (NOW + 3600)).abs() <= 1);
    }

    #[test]
    fn a_run_out_from_the_window_average_is_marked() {
        let early = forecast(&session(50.0), &[at(NOW - 600, 45.0), at(NOW, 50.0)], NOW);
        assert!(matches!(
            early,
            Some(Forecast::RunsOut {
                from_average: true,
                ..
            })
        ));
        let trend = forecast(&session(50.0), &[at(NOW - 3000, 20.0), at(NOW, 50.0)], NOW);
        assert!(matches!(
            trend,
            Some(Forecast::RunsOut {
                from_average: false,
                ..
            })
        ));
    }

    #[test]
    fn no_forecast_too_early_when_full_or_after_the_reset() {
        let young = Quota {
            resets_at: NOW + SESSION_SECS as i64 - 1000,
            ..session(10.0)
        };
        assert_eq!(forecast(&young, &[], NOW), None);
        assert_eq!(forecast(&session(100.0), &[], NOW), None);
        assert_eq!(forecast(&session(112.0), &[], NOW), None);
        let over = Quota {
            resets_at: NOW,
            ..session(50.0)
        };
        assert_eq!(forecast(&over, &[], NOW), None);
    }

    #[test]
    fn weekly_uses_a_one_day_lookback() {
        // Window 3 days old; 30 % a day ago, 50 % now → 20 %/day → 100 % in 2.5 days,
        // before the reset 4 days out.
        let q = Quota {
            key: "weekly".into(),
            label: "Weekly".into(),
            percent: 50.0,
            resets_at: NOW + 4 * 86_400,
            period_secs: WEEKLY_SECS,
        };
        let samples = [at(NOW - 86_400, 30.0), at(NOW - 3600, 49.0), at(NOW, 50.0)];
        let f = forecast(&q, &samples, NOW);
        assert!((runs_out_at(f) - (NOW + 216_000)).abs() <= 1);
    }

    #[test]
    fn for_quotas_covers_session_and_weekly_only() {
        let mut h = History::default();
        h.by_key
            .insert("session".into(), vec![at(NOW - 3000, 20.0), at(NOW, 50.0)]);
        let scoped = Quota {
            key: "weekly:fable".into(),
            ..session(50.0)
        };
        let out = for_quotas(&[session(50.0), scoped], &h, NOW);
        assert_eq!(out.len(), 1);
        assert!(matches!(out["session"], Forecast::RunsOut { .. }));
    }

    #[test]
    fn for_quotas_skips_an_absent_quota_even_with_old_samples() {
        // No session running: the response has no session quota, history.json still does.
        let mut h = History::default();
        h.by_key.insert(
            "session".into(),
            vec![at(NOW - 3000, 20.0), at(NOW - 60, 50.0)],
        );
        assert!(for_quotas(&[], &h, NOW).is_empty());
    }

    #[test]
    fn serializes_with_a_kind_tag() {
        let v = serde_json::to_value(Forecast::RunsOut {
            at: 5,
            from_average: true,
        })
        .expect("serializes");
        assert_eq!(v, serde_json::json!({"kind": "runs_out", "at": 5}));
        let v = serde_json::to_value(Forecast::AtReset { percent: 78.0 }).expect("serializes");
        assert_eq!(v, serde_json::json!({"kind": "at_reset", "percent": 78.0}));
    }
}
