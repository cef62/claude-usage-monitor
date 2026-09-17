//! Poll loop: owns the token for the duration of one request, applies the rate-limit
//! discipline, and publishes a `Snapshot` for the tray and the popover.

use crate::usage::{self, FetchError, Quota, USAGE_BASE_URL};
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const BASE_INTERVAL: u64 = 180;
pub const COOLDOWN: u64 = 120;
pub const ERROR_RETRY: u64 = 30;
pub const MAX_BACKOFF: u64 = 900;
pub const RESET_GRACE: u64 = 5;
/// Sleep slice; each slice re-renders the tray title so the countdown ticks.
pub const TITLE_TICK: u64 = 60;

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
    pub fetched_at: Option<i64>,
    pub next_poll_at: i64,
    pub status: Status,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            quotas: Vec::new(),
            fetched_at: None,
            next_poll_at: 0,
            status: Status::NoToken,
        }
    }
}

pub type Shared = Arc<Mutex<Snapshot>>;

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
) -> u64 {
    match outcome {
        Outcome::Success => match nearest_reset {
            Some(reset) if reset > now && (reset - now) as u64 + RESET_GRACE < BASE_INTERVAL => {
                (reset - now) as u64 + RESET_GRACE
            }
            _ => BASE_INTERVAL,
        },
        Outcome::Unauthorized | Outcome::Failed => ERROR_RETRY,
        Outcome::RateLimited {
            retry_after: Some(secs),
        } => (*secs).clamp(BASE_INTERVAL, MAX_BACKOFF),
        Outcome::RateLimited { retry_after: None } => {
            let shift = error_count.saturating_sub(1).min(4);
            (BASE_INTERVAL << shift).min(MAX_BACKOFF)
        }
    }
}

pub fn run<F: Fn(&Snapshot) + Send + 'static>(shared: Shared, on_update: F) {
    std::thread::spawn(move || {
        let client = usage::client();
        let user_agent = usage::user_agent();
        let mut error_count: u32 = 0;
        let mut last_success: Option<i64> = None;
        let mut latched_fingerprint: Option<u64> = None;

        loop {
            let now = now();
            // Clock-jump guard: a backwards jump must not freeze polling.
            // ponytail: the cooldown check runs before the reset-aligned delay is honored, so a
            // reset less than 120s after a successful fetch is observed on the next regular poll
            // instead. Upgrade path: skip the cooldown when the previous outcome was a
            // reset-aligned poll.
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
                    Outcome::Unauthorized
                }
                Some(creds) => {
                    match usage::fetch_usage(&client, USAGE_BASE_URL, &creds.token, &user_agent) {
                        Ok(body) => {
                            let quotas = usage::normalize(&body);
                            error_count = 0;
                            latched_fingerprint = None;
                            last_success = Some(now);
                            let mut s = lock(&shared);
                            if !quotas.is_empty() {
                                s.quotas = quotas;
                            }
                            s.fetched_at = Some(now);
                            s.status = Status::Ok;
                            Outcome::Success
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
            let delay = next_delay(&outcome, error_count, nearest_reset, now);
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
        assert_eq!(next_delay(&Outcome::Success, 0, None, NOW), BASE_INTERVAL);
        assert_eq!(
            next_delay(&Outcome::Success, 0, Some(NOW + 500), NOW),
            BASE_INTERVAL
        );
        assert_eq!(
            next_delay(&Outcome::Success, 0, Some(NOW - 10), NOW),
            BASE_INTERVAL
        );
    }

    #[test]
    fn success_aligns_to_an_imminent_reset() {
        assert_eq!(
            next_delay(&Outcome::Success, 0, Some(NOW + 60), NOW),
            60 + RESET_GRACE
        );
    }

    #[test]
    fn failures_retry_quickly() {
        assert_eq!(next_delay(&Outcome::Failed, 3, None, NOW), ERROR_RETRY);
        assert_eq!(
            next_delay(&Outcome::Unauthorized, 0, None, NOW),
            ERROR_RETRY
        );
    }

    #[test]
    fn rate_limit_honors_retry_after_within_bounds() {
        let rl = |s| Outcome::RateLimited {
            retry_after: Some(s),
        };
        assert_eq!(next_delay(&rl(10), 1, None, NOW), BASE_INTERVAL);
        assert_eq!(next_delay(&rl(400), 1, None, NOW), 400);
        assert_eq!(next_delay(&rl(2000), 1, None, NOW), MAX_BACKOFF);
    }

    #[test]
    fn rate_limit_backs_off_exponentially_without_retry_after() {
        let rl = Outcome::RateLimited { retry_after: None };
        assert_eq!(next_delay(&rl, 1, None, NOW), 180);
        assert_eq!(next_delay(&rl, 2, None, NOW), 360);
        assert_eq!(next_delay(&rl, 3, None, NOW), 720);
        assert_eq!(next_delay(&rl, 4, None, NOW), MAX_BACKOFF);
        assert_eq!(next_delay(&rl, 40, None, NOW), MAX_BACKOFF);
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
    }
}
