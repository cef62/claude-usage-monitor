//! Per-window usage history for the sparkline: timestamps and percentages only.

use crate::usage::Quota;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

// 7 d at the 120 s poll floor = 5040 samples; keep the whole weekly window.
pub const MAX_SAMPLES: usize = 5100;
pub const MIN_GAP_SECS: i64 = 60;
pub const POPOVER_POINTS: usize = 200;
const FILE_NAME: &str = "history.json";

/// The quotas that get a history (and so a sparkline and a forecast); per-model quotas do not.
pub fn tracked(key: &str) -> bool {
    key == "session" || key == "weekly"
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub t: i64,
    pub pct: f32,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct History {
    pub by_key: HashMap<String, Vec<Sample>>,
}

impl History {
    /// Records the session/weekly quotas at `now`. Samples from before each quota's window
    /// start are dropped, so a reset empties the line without any reset detection. Returns
    /// whether anything changed, so the caller writes the file only when needed.
    pub fn record(&mut self, quotas: &[Quota], now: i64) -> bool {
        let mut changed = false;
        for q in quotas.iter().filter(|q| tracked(&q.key)) {
            let window_start = q.resets_at - q.period_secs as i64;
            let samples = self.by_key.entry(q.key.clone()).or_default();
            let before = samples.len();
            samples.retain(|s| s.t >= window_start);
            changed |= samples.len() != before;
            let too_soon = samples
                .last()
                .is_some_and(|last| now - last.t < MIN_GAP_SECS);
            if !too_soon {
                samples.push(Sample {
                    t: now,
                    pct: q.percent as f32,
                });
                changed = true;
            }
            if samples.len() > MAX_SAMPLES {
                let excess = samples.len() - MAX_SAMPLES;
                samples.drain(..excess);
                changed = true;
            }
        }
        changed
    }

    pub fn for_popover(&self) -> HashMap<String, Vec<Sample>> {
        self.by_key
            .iter()
            .map(|(k, v)| (k.clone(), downsample(v, POPOVER_POINTS)))
            .collect()
    }
}

/// Even-stride thinning that always keeps the newest sample.
pub fn downsample(samples: &[Sample], max: usize) -> Vec<Sample> {
    if max == 0 {
        return Vec::new();
    }
    if max == 1 {
        return samples.last().map(|s| vec![*s]).unwrap_or_default();
    }
    if samples.len() <= max {
        return samples.to_vec();
    }
    let last = samples.len() - 1;
    (0..max).map(|i| samples[i * last / (max - 1)]).collect()
}

pub fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FILE_NAME))
}

pub fn load(path: &Path) -> History {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, h: &History) -> std::io::Result<()> {
    let json = serde_json::to_vec(h).map_err(std::io::Error::other)?;
    crate::settings::write_atomic(path, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn quotas() -> Vec<Quota> {
        vec![
            quota("session", 40.0, 3600, SESSION_SECS),
            quota("weekly", 60.0, 3 * 86400, WEEKLY_SECS),
            quota("weekly:fable", 99.0, 3 * 86400, WEEKLY_SECS),
        ]
    }

    #[test]
    fn tracks_session_and_weekly_only() {
        assert!(tracked("session"));
        assert!(tracked("weekly"));
        assert!(!tracked("weekly:fable"));
    }

    #[test]
    fn records_session_and_weekly_only() {
        let mut h = History::default();
        assert!(h.record(&quotas(), NOW));
        assert_eq!(h.by_key.len(), 2);
        assert_eq!(h.by_key["session"], vec![Sample { t: NOW, pct: 40.0 }]);
        assert_eq!(h.by_key["weekly"].len(), 1);
        assert!(!h.by_key.contains_key("weekly:fable"));
    }

    #[test]
    fn one_sample_per_minute() {
        let mut h = History::default();
        h.record(&quotas(), NOW);
        assert!(!h.record(&quotas(), NOW + 30));
        assert_eq!(h.by_key["session"].len(), 1);
        assert!(h.record(&quotas(), NOW + 60));
        assert_eq!(h.by_key["session"].len(), 2);
    }

    #[test]
    fn prunes_samples_before_the_window_start() {
        let mut h = History::default();
        // window start = NOW + 3600 − 18000 = NOW − 14400
        h.by_key.insert(
            "session".into(),
            vec![
                Sample {
                    t: NOW - 20_000,
                    pct: 90.0,
                },
                Sample {
                    t: NOW - 10_000,
                    pct: 10.0,
                },
            ],
        );
        assert!(h.record(&quotas(), NOW));
        let s = &h.by_key["session"];
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].t, NOW - 10_000);
        assert_eq!(s[1].t, NOW);
    }

    #[test]
    fn caps_at_max_samples() {
        let mut h = History::default();
        let window_start = NOW + 3600 - SESSION_SECS as i64;
        let old: Vec<Sample> = (0..MAX_SAMPLES as i64)
            .map(|i| Sample {
                t: window_start + i,
                pct: 1.0,
            })
            .collect();
        h.by_key.insert("session".into(), old);
        assert!(h.record(&quotas(), NOW));
        let s = &h.by_key["session"];
        assert_eq!(s.len(), MAX_SAMPLES);
        assert_eq!(s[0].t, window_start + 1);
        assert_eq!(s[MAX_SAMPLES - 1].t, NOW);
    }

    #[test]
    fn downsample_keeps_the_last_sample() {
        let all: Vec<Sample> = (0..1000).map(|i| Sample { t: i, pct: 0.0 }).collect();
        let d = downsample(&all, 200);
        assert_eq!(d.len(), 200);
        assert_eq!(d[0].t, 0);
        assert_eq!(d[199].t, 999);
        let few: Vec<Sample> = (0..50).map(|i| Sample { t: i, pct: 0.0 }).collect();
        assert_eq!(downsample(&few, 200), few);
    }

    #[test]
    fn for_popover_downsamples_each_key() {
        let mut h = History::default();
        h.by_key.insert(
            "weekly".into(),
            (0..3000).map(|i| Sample { t: i, pct: 0.0 }).collect(),
        );
        assert_eq!(h.for_popover()["weekly"].len(), POPOVER_POINTS);
    }

    #[test]
    fn save_and_load_round_trip_and_garbage_is_empty() {
        let dir = std::env::temp_dir().join(format!("cum-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("history.json");
        let mut h = History::default();
        h.record(&quotas(), NOW);
        save(&path, &h).expect("save");
        assert_eq!(load(&path), h);
        std::fs::write(&path, b"{not json").expect("write");
        assert_eq!(load(&path), History::default());
        assert_eq!(load(&dir.join("missing.json")), History::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn downsample_handles_tiny_max() {
        let all: Vec<Sample> = (0..1000).map(|i| Sample { t: i, pct: 0.0 }).collect();
        let d = downsample(&all, 1);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].t, 999);
        let e = downsample(&all, 0);
        assert_eq!(e.len(), 0);
        let empty: Vec<Sample> = vec![];
        let f = downsample(&empty, 1);
        assert_eq!(f.len(), 0);
    }
}
