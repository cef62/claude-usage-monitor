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

#[cfg(test)]
mod tests {
    use super::*;

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
}
