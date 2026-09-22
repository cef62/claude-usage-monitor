//! Poll loop: owns the token for the duration of one request, applies the rate-limit
//! discipline, and publishes a `Snapshot` for the tray and the popover.

use crate::forecast::{self, Forecast};
use crate::history;
use crate::log;
use crate::settings::{Settings, MAX_POLL_SECS, MIN_POLL_SECS};
use crate::usage::{self, FetchError, Quota, USAGE_BASE_URL};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const BASE_INTERVAL: u64 = 180;
pub const COOLDOWN: u64 = 120;
pub const ERROR_RETRY: u64 = 30;
pub const MAX_BACKOFF: u64 = 900;
pub const RESET_GRACE: u64 = 5;
/// Sleep slice; each slice re-renders the tray title so the countdown ticks.
pub const TITLE_TICK: u64 = 60;

const _: () = assert!(MAX_POLL_SECS <= MAX_BACKOFF);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Status {
    Ok,
    NoToken,
    AuthExpired,
    RateLimited { until: i64 },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    /// Last good quotas. An error never empties this.
    pub quotas: Vec<Quota>,
    /// Plan name from the profile endpoint, fetched once per token.
    pub plan: Option<String>,
    /// Overage credits from the usage response, when the account has extra usage enabled.
    pub extra: Option<usage::ExtraUsage>,
    /// Downsampled per-window samples for the popover sparkline.
    pub history: HashMap<String, Vec<history::Sample>>,
    /// Pace forecast per session/weekly quota, computed from the full-resolution history.
    pub forecast: HashMap<String, Forecast>,
    pub fetched_at: Option<i64>,
    pub next_poll_at: i64,
    pub status: Status,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            quotas: Vec::new(),
            plan: None,
            extra: None,
            history: HashMap::new(),
            forecast: HashMap::new(),
            fetched_at: None,
            next_poll_at: 0,
            status: Status::NoToken,
        }
    }
}

pub type Shared = Arc<Mutex<Snapshot>>;

/// The profile is re-read only when the token changes (or was never read).
pub fn needs_profile(profile_for: Option<u64>, fingerprint: u64) -> bool {
    profile_for != Some(fingerprint)
}

fn lock(shared: &Shared) -> MutexGuard<'_, Snapshot> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Sleeps `secs` in TITLE_TICK slices, publishing the snapshot after each so the tray
/// countdown keeps ticking without network traffic.
fn sleep_ticking<F: Fn(&Snapshot)>(secs: u64, shared: &Shared, on_update: &F) {
    let mut remaining = secs;
    while remaining > 0 {
        let step = remaining.min(TITLE_TICK);
        std::thread::sleep(Duration::from_secs(step));
        remaining -= step;
        on_update(&read(shared));
    }
}

pub fn read(shared: &Shared) -> Snapshot {
    lock(shared).clone()
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub enum Outcome {
    Success,
    Unauthorized,
    RateLimited { retry_after: Option<u64> },
    Failed,
}

pub fn next_delay(
    outcome: &Outcome,
    error_count: u32,
    nearest_reset: Option<i64>,
    now: i64,
    base: u64,
) -> u64 {
    match outcome {
        Outcome::Success => match nearest_reset {
            Some(reset) if reset > now && (reset - now) as u64 + RESET_GRACE < base => {
                ((reset - now) as u64 + RESET_GRACE).max(COOLDOWN)
            }
            _ => base,
        },
        Outcome::Unauthorized | Outcome::Failed => ERROR_RETRY,
        Outcome::RateLimited {
            retry_after: Some(secs),
        } => (*secs).clamp(base, MAX_BACKOFF),
        Outcome::RateLimited { retry_after: None } => {
            let shift = error_count.saturating_sub(1).min(4);
            (base << shift).min(MAX_BACKOFF)
        }
    }
}

/// One line per cycle for the local log. Percentages only; never anything from the request.
pub fn log_line(outcome: &Outcome, snapshot: &Snapshot, delay: u64) -> String {
    match &snapshot.status {
        Status::Ok if matches!(outcome, Outcome::Success) => {
            let pct = |key: &str| {
                snapshot
                    .quotas
                    .iter()
                    .find(|q| q.key == key)
                    .map(|q| format!("{}%", q.percent.round() as i64))
                    .unwrap_or_else(|| "-".to_string())
            };
            format!(
                "poll ok session={} weekly={} next={delay}s",
                pct("session"),
                pct("weekly")
            )
        }
        Status::RateLimited { .. } => format!("poll rate_limited retry={delay}s"),
        Status::AuthExpired => "poll auth_expired".to_string(),
        Status::NoToken => "poll no_token".to_string(),
        Status::Error { message } => format!("poll error {message}"),
        Status::Ok => format!("poll ok next={delay}s"),
    }
}

pub fn run<F: Fn(&Snapshot) + Send + 'static>(
    shared: Shared,
    settings: Arc<Mutex<Settings>>,
    log_path: Option<PathBuf>,
    history_path: Option<PathBuf>,
    on_update: F,
) {
    std::thread::spawn(move || {
        let client = usage::client();
        let user_agent = usage::user_agent();
        let mut error_count: u32 = 0;
        let mut last_success: Option<i64> = None;
        let mut latched_fingerprint: Option<u64> = None;
        let mut profile_for: Option<u64> = None;
        let mut history = history_path
            .as_deref()
            .map(history::load)
            .unwrap_or_default();

        loop {
            let now = now();
            // Clock-jump guard: a backwards jump must not freeze polling.
            // ponytail: next_delay now floors the reset-aligned result at COOLDOWN, so this
            // cooldown check is a safety net for backwards clock jumps rather than the primary
            // enforcement. Upgrade path: skip it entirely once next_delay is trusted alone.
            if let Some(last) = last_success.filter(|last| now >= *last) {
                let since = (now - last) as u64;
                if since < COOLDOWN {
                    sleep_ticking(COOLDOWN - since, &shared, &on_update);
                    continue;
                }
            }

            let outcome = match usage::read_credentials() {
                None => {
                    lock(&shared).status = Status::NoToken;
                    Outcome::Failed
                }
                Some(creds) if latched_fingerprint == Some(creds.fingerprint) => {
                    // Known-bad token: wait for the credential store to change.
                    // Re-assert AuthExpired: a NoToken cycle in between could have overwritten it.
                    lock(&shared).status = Status::AuthExpired;
                    Outcome::Unauthorized
                }
                Some(creds) => {
                    match usage::fetch_usage(&client, USAGE_BASE_URL, &creds.token, &user_agent) {
                        Ok(body) => {
                            let quotas = usage::normalize(&body);
                            if quotas.is_empty() {
                                error_count += 1;
                                lock(&shared).status = Status::Error {
                                    message: "no quotas in response".to_string(),
                                };
                                Outcome::Failed
                            } else {
                                error_count = 0;
                                latched_fingerprint = None;
                                last_success = Some(now);
                                let extra = usage::extra_usage(&body);
                                // One profile call per token: it only carries the plan name.
                                let plan = if needs_profile(profile_for, creds.fingerprint) {
                                    match usage::fetch_profile(
                                        &client,
                                        USAGE_BASE_URL,
                                        &creds.token,
                                        &user_agent,
                                    ) {
                                        Ok(profile) => {
                                            profile_for = Some(creds.fingerprint);
                                            usage::plan_label(&profile)
                                        }
                                        Err(e) => {
                                            if let Some(path) = &log_path {
                                                let _ = log::append(
                                                    path,
                                                    &format!("profile failed {e:?}"),
                                                );
                                            }
                                            None
                                        }
                                    }
                                } else {
                                    None
                                };
                                if history.record(&quotas, now) {
                                    if let Some(path) = &history_path {
                                        // In-memory history is authoritative; a failed write only
                                        // loses persistence across restarts.
                                        let _ = history::save(path, &history);
                                    }
                                }
                                let popover_history = history.for_popover();
                                let forecasts = forecast::for_quotas(&quotas, &history, now);
                                let mut s = lock(&shared);
                                s.quotas = quotas;
                                s.history = popover_history;
                                s.forecast = forecasts;
                                s.extra = extra;
                                if plan.is_some() {
                                    s.plan = plan;
                                }
                                s.fetched_at = Some(now);
                                s.status = Status::Ok;
                                Outcome::Success
                            }
                        }
                        Err(FetchError::Unauthorized) => {
                            latched_fingerprint = Some(creds.fingerprint);
                            lock(&shared).status = Status::AuthExpired;
                            Outcome::Unauthorized
                        }
                        Err(FetchError::RateLimited { retry_after }) => {
                            error_count += 1;
                            Outcome::RateLimited { retry_after }
                        }
                        Err(FetchError::Server(code)) => {
                            error_count += 1;
                            lock(&shared).status = Status::Error {
                                message: format!("HTTP {code}"),
                            };
                            Outcome::Failed
                        }
                        Err(FetchError::Network(message)) => {
                            error_count += 1;
                            lock(&shared).status = Status::Error { message };
                            Outcome::Failed
                        }
                    }
                }
            };

            let nearest_reset = lock(&shared)
                .quotas
                .iter()
                .map(|q| q.resets_at)
                .filter(|r| *r > now)
                .min();
            let base = settings
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .poll_interval_secs
                .clamp(MIN_POLL_SECS, MAX_POLL_SECS);
            let delay = next_delay(&outcome, error_count, nearest_reset, now, base);
            {
                let mut s = lock(&shared);
                if let Outcome::RateLimited { .. } = outcome {
                    s.status = Status::RateLimited {
                        until: now + delay as i64,
                    };
                }
                s.next_poll_at = now + delay as i64;
            }
            on_update(&read(&shared));
            if let Some(path) = &log_path {
                let _ = log::append(path, &log_line(&outcome, &read(&shared), delay));
            }

            sleep_ticking(delay, &shared, &on_update);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_789_588_800;

    #[test]
    fn success_uses_base_interval_without_imminent_reset() {
        assert_eq!(
            next_delay(&Outcome::Success, 0, None, NOW, BASE_INTERVAL),
            BASE_INTERVAL
        );
        assert_eq!(
            next_delay(&Outcome::Success, 0, Some(NOW + 500), NOW, BASE_INTERVAL),
            BASE_INTERVAL
        );
        assert_eq!(
            next_delay(&Outcome::Success, 0, Some(NOW - 10), NOW, BASE_INTERVAL),
            BASE_INTERVAL
        );
    }

    #[test]
    fn success_aligns_to_an_imminent_reset() {
        assert_eq!(
            next_delay(&Outcome::Success, 0, Some(NOW + 60), NOW, BASE_INTERVAL),
            COOLDOWN
        );
        assert_eq!(
            next_delay(&Outcome::Success, 0, Some(NOW + 150), NOW, BASE_INTERVAL),
            150 + RESET_GRACE
        );
    }

    #[test]
    fn failures_retry_quickly() {
        assert_eq!(
            next_delay(&Outcome::Failed, 3, None, NOW, BASE_INTERVAL),
            ERROR_RETRY
        );
        assert_eq!(
            next_delay(&Outcome::Unauthorized, 0, None, NOW, BASE_INTERVAL),
            ERROR_RETRY
        );
    }

    #[test]
    fn rate_limit_honors_retry_after_within_bounds() {
        let rl = |s| Outcome::RateLimited {
            retry_after: Some(s),
        };
        assert_eq!(
            next_delay(&rl(10), 1, None, NOW, BASE_INTERVAL),
            BASE_INTERVAL
        );
        assert_eq!(next_delay(&rl(400), 1, None, NOW, BASE_INTERVAL), 400);
        assert_eq!(
            next_delay(&rl(2000), 1, None, NOW, BASE_INTERVAL),
            MAX_BACKOFF
        );
    }

    #[test]
    fn rate_limit_backs_off_exponentially_without_retry_after() {
        let rl = Outcome::RateLimited { retry_after: None };
        assert_eq!(next_delay(&rl, 1, None, NOW, BASE_INTERVAL), 180);
        assert_eq!(next_delay(&rl, 2, None, NOW, BASE_INTERVAL), 360);
        assert_eq!(next_delay(&rl, 3, None, NOW, BASE_INTERVAL), 720);
        assert_eq!(next_delay(&rl, 4, None, NOW, BASE_INTERVAL), MAX_BACKOFF);
        assert_eq!(next_delay(&rl, 40, None, NOW, BASE_INTERVAL), MAX_BACKOFF);
    }

    #[test]
    fn success_uses_the_configured_base() {
        assert_eq!(next_delay(&Outcome::Success, 0, None, NOW, 300), 300);
        assert_eq!(
            next_delay(&Outcome::Success, 0, Some(NOW + 60), NOW, 300),
            COOLDOWN
        );
        assert_eq!(
            next_delay(&Outcome::Success, 0, Some(NOW + 250), NOW, 300),
            255
        );
    }

    #[test]
    fn rate_limit_backoff_starts_from_base() {
        let rl = Outcome::RateLimited { retry_after: None };
        assert_eq!(next_delay(&rl, 1, None, NOW, 600), 600);
        assert_eq!(next_delay(&rl, 3, None, NOW, 600), MAX_BACKOFF);
        let ra = Outcome::RateLimited {
            retry_after: Some(200),
        };
        assert_eq!(next_delay(&ra, 1, None, NOW, 300), 300);
    }

    #[test]
    fn log_line_summarizes_the_cycle() {
        let snap = Snapshot {
            plan: None,
            extra: None,
            history: HashMap::new(),
            forecast: HashMap::new(),
            quotas: vec![
                Quota {
                    key: "session".into(),
                    label: "Session".into(),
                    percent: 48.4,
                    resets_at: NOW + 100,
                    period_secs: 18000,
                },
                Quota {
                    key: "weekly".into(),
                    label: "Weekly".into(),
                    percent: 64.0,
                    resets_at: NOW + 100,
                    period_secs: 604800,
                },
            ],
            fetched_at: Some(NOW),
            next_poll_at: NOW + 180,
            status: Status::Ok,
        };
        assert_eq!(
            log_line(&Outcome::Success, &snap, 180),
            "poll ok session=48% weekly=64% next=180s"
        );
        let mut rl = snap.clone();
        rl.status = Status::RateLimited { until: NOW + 360 };
        assert_eq!(
            log_line(&Outcome::RateLimited { retry_after: None }, &rl, 360),
            "poll rate_limited retry=360s"
        );
        let mut err = snap.clone();
        err.status = Status::Error {
            message: "HTTP 500".into(),
        };
        assert_eq!(log_line(&Outcome::Failed, &err, 30), "poll error HTTP 500");
        let mut nt = snap.clone();
        nt.status = Status::NoToken;
        assert_eq!(log_line(&Outcome::Failed, &nt, 30), "poll no_token");
        let mut ae = snap;
        ae.status = Status::AuthExpired;
        assert_eq!(
            log_line(&Outcome::Unauthorized, &ae, 30),
            "poll auth_expired"
        );
    }

    #[test]
    fn status_serializes_with_kind_tag() {
        let ok = serde_json::to_value(Status::Ok).expect("serializes");
        assert_eq!(ok, serde_json::json!({"kind": "ok"}));
        let rl = serde_json::to_value(Status::RateLimited { until: 5 }).expect("serializes");
        assert_eq!(rl, serde_json::json!({"kind": "rate_limited", "until": 5}));
        let err = serde_json::to_value(Status::Error {
            message: "HTTP 500".into(),
        })
        .expect("serializes");
        assert_eq!(
            err,
            serde_json::json!({"kind": "error", "message": "HTTP 500"})
        );
    }

    #[test]
    fn default_snapshot_has_no_token_and_no_quotas() {
        let s = Snapshot::default();
        assert_eq!(s.status, Status::NoToken);
        assert!(s.quotas.is_empty());
        assert_eq!(s.fetched_at, None);
        assert!(s.forecast.is_empty());
    }

    #[test]
    fn profile_is_fetched_once_per_token() {
        assert!(needs_profile(None, 7));
        assert!(needs_profile(Some(1), 7));
        assert!(!needs_profile(Some(7), 7));
    }
}
