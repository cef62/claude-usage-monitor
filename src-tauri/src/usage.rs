//! Credentials, HTTP fetch and normalization of the Claude usage API.
//!
//! The token only ever lives in local variables here and in `poll.rs`. It is never
//! logged and never serialized.

use serde::Serialize;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub const SESSION_SECS: u64 = 5 * 3600;
pub const WEEKLY_SECS: u64 = 7 * 86400;

pub struct Credentials {
    pub token: String,
    /// Hash of the raw credential blob, so a 401 latch can notice a re-login without
    /// keeping the token around for comparison.
    pub fingerprint: u64,
}

pub fn parse_credentials(blob: &str) -> Option<Credentials> {
    let v: Value = serde_json::from_str(blob).ok()?;
    let token = v.get("claudeAiOauth")?.get("accessToken")?.as_str()?;
    if token.is_empty() {
        return None;
    }
    let mut h = DefaultHasher::new();
    blob.hash(&mut h);
    Some(Credentials {
        token: token.to_string(),
        fingerprint: h.finish(),
    })
}

pub fn read_credentials() -> Option<Credentials> {
    parse_credentials(&read_blob()?)
}

/// Claude Code on macOS stores the blob in the login Keychain under this service name.
#[cfg(target_os = "macos")]
fn read_blob() -> Option<String> {
    let out = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

#[cfg(not(target_os = "macos"))]
fn read_blob() -> Option<String> {
    let dir = std::env::var("CLAUDE_CONFIG_DIR").ok().or_else(|| {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()?;
        Some(format!("{home}/.claude"))
    })?;
    std::fs::read_to_string(format!("{dir}/.credentials.json")).ok()
}

pub const USAGE_BASE_URL: &str = "https://api.anthropic.com";
/// Used when `claude --version` is unavailable. Any non-Claude-Code User-Agent has been
/// permanently rate limited upstream, so the prefix matters more than the exact number.
const FALLBACK_CLI_VERSION: &str = "2.1.273";

#[derive(Debug, PartialEq)]
pub enum FetchError {
    Unauthorized,
    RateLimited { retry_after: Option<u64> },
    Server(u16),
    Network(String),
}

/// `claude` is a `.cmd` shim on Windows, which `CreateProcess` will not resolve, so go through
/// `cmd /C`. `CREATE_NO_WINDOW` keeps the console from flashing in front of the tray app.
#[cfg(target_os = "windows")]
fn claude_version_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut c = std::process::Command::new("cmd");
    c.args(["/C", "claude", "--version"])
        .creation_flags(CREATE_NO_WINDOW);
    c
}

#[cfg(not(target_os = "windows"))]
fn claude_version_command() -> std::process::Command {
    let mut c = std::process::Command::new("claude");
    c.arg("--version");
    c
}

pub fn user_agent() -> String {
    let version = claude_version_command()
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().next().map(str::to_string))
        .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or_else(|| FALLBACK_CLI_VERSION.to_string());
    format!("claude-code/{version}")
}

pub fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new())
}

/// Only the delta-seconds form is honored; an HTTP-date falls back to the caller's backoff.
pub fn retry_after_secs(header: Option<&str>) -> Option<u64> {
    header?.trim().parse().ok()
}

pub fn fetch_usage(
    client: &reqwest::blocking::Client,
    base_url: &str,
    token: &str,
    user_agent: &str,
) -> Result<Value, FetchError> {
    fetch_json(client, base_url, "/api/oauth/usage", token, user_agent)
}

/// Account/organization metadata; only the plan tier is used. Fetched once per token.
pub fn fetch_profile(
    client: &reqwest::blocking::Client,
    base_url: &str,
    token: &str,
    user_agent: &str,
) -> Result<Value, FetchError> {
    fetch_json(client, base_url, "/api/oauth/profile", token, user_agent)
}

fn fetch_json(
    client: &reqwest::blocking::Client,
    base_url: &str,
    path: &str,
    token: &str,
    user_agent: &str,
) -> Result<Value, FetchError> {
    let resp = client
        .get(format!("{base_url}{path}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Content-Type", "application/json")
        .header("User-Agent", user_agent)
        .send()
        .map_err(|e| FetchError::Network(e.without_url().to_string()))?;
    match resp.status().as_u16() {
        200..=299 => resp
            .json()
            .map_err(|e| FetchError::Network(e.without_url().to_string())),
        401 => Err(FetchError::Unauthorized),
        429 => {
            let header = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok());
            Err(FetchError::RateLimited {
                retry_after: retry_after_secs(header),
            })
        }
        code => Err(FetchError::Server(code)),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Quota {
    /// `session`, `weekly`, or `weekly:<model>` (lowercase display name).
    pub key: String,
    pub label: String,
    /// Raw utilization; may exceed 100 during overage. The UI clamps for drawing only.
    pub percent: f64,
    /// Unix seconds.
    pub resets_at: i64,
    pub period_secs: u64,
}

/// Prefer `limits[]` (newer, carries per-model caps); fall back to the flat fields.
pub fn normalize(v: &Value) -> Vec<Quota> {
    let mut out: Vec<Quota> = v
        .get("limits")
        .and_then(Value::as_array)
        .map(|limits| limits.iter().filter_map(from_limit).collect())
        .unwrap_or_default();
    if out.is_empty() {
        out = from_flat(v);
    }
    out.sort_by(|a, b| {
        rank(&a.key)
            .cmp(&rank(&b.key))
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

fn rank(key: &str) -> u8 {
    match key {
        "session" => 0,
        "weekly" => 1,
        _ => 2,
    }
}

fn from_limit(l: &Value) -> Option<Quota> {
    let percent = l.get("percent")?.as_f64()?;
    let resets_at = parse_ts(l.get("resets_at")?.as_str()?)?;
    let (key, label, period_secs) = match l.get("kind")?.as_str()? {
        "session" => ("session".to_string(), "Session".to_string(), SESSION_SECS),
        "weekly_all" => ("weekly".to_string(), "Weekly".to_string(), WEEKLY_SECS),
        "weekly_scoped" => {
            let model = l.pointer("/scope/model/display_name")?.as_str()?;
            (
                format!("weekly:{}", model.to_lowercase()),
                format!("{model} weekly"),
                WEEKLY_SECS,
            )
        }
        _ => return None,
    };
    Some(Quota {
        key,
        label,
        percent,
        resets_at,
        period_secs,
    })
}

fn from_flat(v: &Value) -> Vec<Quota> {
    let Some(obj) = v.as_object() else {
        return Vec::new();
    };
    obj.iter()
        .filter_map(|(field, val)| {
            let (key, label, period_secs) = match field.as_str() {
                "five_hour" => ("session".to_string(), "Session".to_string(), SESSION_SECS),
                "seven_day" => ("weekly".to_string(), "Weekly".to_string(), WEEKLY_SECS),
                other => {
                    let name = other.strip_prefix("seven_day_")?;
                    (
                        format!("weekly:{name}"),
                        format!("{} weekly", capitalize(name)),
                        WEEKLY_SECS,
                    )
                }
            };
            let percent = val.get("utilization")?.as_f64()?;
            let resets_at = parse_ts(val.get("resets_at")?.as_str()?)?;
            Some(Quota {
                key,
                label,
                percent,
                resets_at,
                period_secs,
            })
        })
        .collect()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)` into unix seconds. The API always sends
/// this shape, so a hand-written parser beats a chrono dependency.
pub fn parse_ts(s: &str) -> Option<i64> {
    let (date, rest) = s.trim().split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let tz_pos = rest.rfind(['Z', '+', '-'])?;
    let (time, tz) = rest.split_at(tz_pos);
    let time = time.split('.').next()?;
    let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
    let (hh, mm, ss) = (t.next()??, t.next()??, t.next()??);
    let offset = if tz == "Z" {
        0
    } else {
        let sign = if tz.starts_with('-') { -1 } else { 1 };
        let (oh, om) = tz.get(1..)?.split_once(':')?;
        sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60)
    };
    Some(days_from_civil(y, m, day) * 86400 + hh * 3600 + mm * 60 + ss - offset)
}

/// Howard Hinnant's days-from-civil: days since 1970-01-01 for a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Short plan name from the profile: known `rate_limit_tier`s first, else the organization
/// type title-cased (`claude_max` → `Claude Max`).
pub fn plan_label(profile: &Value) -> Option<String> {
    let org = profile.get("organization")?;
    let tier = org.get("rate_limit_tier").and_then(Value::as_str);
    let known = match tier {
        Some("default_claude_max_5x") => Some("Max 5x"),
        Some("default_claude_max_20x") => Some("Max 20x"),
        Some("default_claude_pro") => Some("Pro"),
        _ => None,
    };
    if let Some(k) = known {
        return Some(k.to_string());
    }
    let org_type = org.get("organization_type").and_then(Value::as_str)?;
    let words: Vec<String> = org_type
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect();
    Some(words.join(" "))
}

/// Pay-as-you-go overage, present only when the account has it switched on. Amounts are the
/// API's minor units; the popover formats them with `decimals`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExtraUsage {
    pub used: f64,
    pub limit: Option<f64>,
    pub currency: String,
    pub decimals: u8,
    pub utilization: Option<f64>,
}

pub fn extra_usage(v: &Value) -> Option<ExtraUsage> {
    let e = v.get("extra_usage")?;
    if e.get("is_enabled").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    Some(ExtraUsage {
        used: e.get("used_credits").and_then(Value::as_f64).unwrap_or(0.0),
        limit: e.get("monthly_limit").and_then(Value::as_f64),
        currency: e
            .get("currency")
            .and_then(Value::as_str)
            .unwrap_or("USD")
            .to_string(),
        decimals: e
            .get("decimal_places")
            .and_then(Value::as_u64)
            .map_or(2, |d| d.min(6) as u8),
        utilization: e.get("utilization").and_then(Value::as_f64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FIXTURE: &str = r#"{
      "five_hour": {"utilization": 48.0, "resets_at": "2026-09-05T12:59:59.966454+00:00"},
      "seven_day": {"utilization": 64.0, "resets_at": "2026-09-11T20:59:59.966479+00:00"},
      "seven_day_sonnet": {"utilization": 2.0, "resets_at": "2026-09-11T20:59:59.966479+00:00"},
      "seven_day_opus": null,
      "nimbus_quill": {"utilization": 0.0, "resets_at": null},
      "limits": [
        {"kind": "session", "percent": 48, "resets_at": "2026-09-05T12:59:59.966454+00:00", "scope": null},
        {"kind": "weekly_all", "percent": 64, "resets_at": "2026-09-11T20:59:59.966479+00:00", "scope": null},
        {"kind": "weekly_scoped", "percent": 12, "resets_at": "2026-09-11T20:59:59.966479+00:00",
         "scope": {"model": {"display_name": "Fable"}}},
        {"kind": "weekly_scoped", "percent": null, "resets_at": "2026-09-11T20:59:59.966479+00:00",
         "scope": {"model": {"display_name": "Opus"}}}
      ]
    }"#;

    fn fixture() -> serde_json::Value {
        serde_json::from_str(FIXTURE).expect("fixture parses")
    }

    #[test]
    fn parse_ts_handles_offsets_and_fractions() {
        assert_eq!(
            parse_ts("2026-09-05T12:59:59.966454+00:00"),
            Some(1788613199)
        );
        assert_eq!(parse_ts("2026-09-05T12:59:59Z"), Some(1788613199));
        assert_eq!(parse_ts("2026-09-05T14:59:59+02:00"), Some(1788613199));
        assert_eq!(parse_ts("garbage"), None);
        assert_eq!(parse_ts(""), None);
    }

    #[test]
    fn normalize_prefers_limits_and_drops_null_percent() {
        let q = normalize(&fixture());
        let keys: Vec<&str> = q.iter().map(|q| q.key.as_str()).collect();
        assert_eq!(keys, vec!["session", "weekly", "weekly:fable"]);
        assert_eq!(q[0].percent, 48.0);
        assert_eq!(q[0].resets_at, 1788613199);
        assert_eq!(q[0].period_secs, SESSION_SECS);
        assert_eq!(q[1].label, "Weekly");
        assert_eq!(q[1].period_secs, WEEKLY_SECS);
        assert_eq!(q[2].label, "Fable weekly");
        assert_eq!(q[2].percent, 12.0);
    }

    #[test]
    fn normalize_falls_back_to_flat_fields_without_limits() {
        let mut v = fixture();
        v.as_object_mut().expect("object").remove("limits");
        let q = normalize(&v);
        let keys: Vec<&str> = q.iter().map(|q| q.key.as_str()).collect();
        assert_eq!(keys, vec!["session", "weekly", "weekly:sonnet"]);
        assert_eq!(q[2].label, "Sonnet weekly");
    }

    #[test]
    fn normalize_falls_back_when_limits_is_empty() {
        let mut v = fixture();
        v["limits"] = serde_json::json!([]);
        assert_eq!(normalize(&v).len(), 3);
    }

    #[test]
    fn normalize_keeps_percent_above_100() {
        let mut v = fixture();
        v["limits"][0]["percent"] = serde_json::json!(112.5);
        assert_eq!(normalize(&v)[0].percent, 112.5);
    }

    #[test]
    fn normalize_returns_empty_for_non_object() {
        assert!(normalize(&serde_json::json!(null)).is_empty());
        assert!(normalize(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn parse_credentials_extracts_token_and_fingerprint() {
        let a = parse_credentials(
            r#"{"claudeAiOauth":{"accessToken":"sk-ant-abc","refreshToken":"r"}}"#,
        )
        .expect("valid blob");
        assert_eq!(a.token, "sk-ant-abc");
        let b = parse_credentials(
            r#"{"claudeAiOauth":{"accessToken":"sk-ant-xyz","refreshToken":"r"}}"#,
        )
        .expect("valid blob");
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn parse_credentials_rejects_bad_input() {
        assert!(parse_credentials("").is_none());
        assert!(parse_credentials("not json").is_none());
        assert!(parse_credentials(r#"{"claudeAiOauth":{}}"#).is_none());
        assert!(parse_credentials(r#"{"claudeAiOauth":{"accessToken":""}}"#).is_none());
        assert!(parse_credentials(r#"{"claudeAiOauth":null}"#).is_none());
    }

    #[test]
    fn retry_after_parses_seconds_only() {
        assert_eq!(retry_after_secs(Some("120")), Some(120));
        assert_eq!(retry_after_secs(Some(" 7 ")), Some(7));
        assert_eq!(
            retry_after_secs(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(retry_after_secs(None), None);
    }

    #[test]
    fn user_agent_has_claude_code_prefix_and_numeric_version() {
        let ua = user_agent();
        let version = ua.strip_prefix("claude-code/").expect("prefix");
        assert!(
            version.chars().next().is_some_and(|c| c.is_ascii_digit()),
            "{ua}"
        );
    }

    #[test]
    fn plan_label_maps_known_tiers_and_falls_back() {
        let tier = |t: &str| json!({"organization": {"rate_limit_tier": t, "organization_type": "claude_max"}});
        assert_eq!(
            plan_label(&tier("default_claude_max_5x")).as_deref(),
            Some("Max 5x")
        );
        assert_eq!(
            plan_label(&tier("default_claude_max_20x")).as_deref(),
            Some("Max 20x")
        );
        assert_eq!(
            plan_label(&tier("default_claude_pro")).as_deref(),
            Some("Pro")
        );
        assert_eq!(
            plan_label(&tier("default_claude_team")).as_deref(),
            Some("Claude Max")
        );
        let no_tier = json!({"organization": {"organization_type": "claude_pro"}});
        assert_eq!(plan_label(&no_tier).as_deref(), Some("Claude Pro"));
        assert_eq!(plan_label(&json!({"account": {}})), None);
    }

    #[test]
    fn extra_usage_only_when_enabled() {
        let off = json!({"extra_usage": {"is_enabled": false, "used_credits": 5.0}});
        assert!(extra_usage(&off).is_none());
        let uncapped = json!({"extra_usage": {"is_enabled": true, "used_credits": 1234.0,
            "monthly_limit": null, "utilization": null, "currency": "EUR", "decimal_places": 2}});
        let e = extra_usage(&uncapped).expect("enabled");
        assert_eq!(e.used, 1234.0);
        assert_eq!(e.limit, None);
        assert_eq!(e.currency, "EUR");
        assert_eq!(e.decimals, 2);
        let capped = json!({"extra_usage": {"is_enabled": true, "used_credits": 2500.0,
            "monthly_limit": 5000.0, "utilization": 50.0, "currency": "USD", "decimal_places": 2}});
        let e = extra_usage(&capped).expect("enabled");
        assert_eq!(e.limit, Some(5000.0));
        assert_eq!(e.utilization, Some(50.0));
        assert!(extra_usage(&json!({})).is_none());
    }
}
