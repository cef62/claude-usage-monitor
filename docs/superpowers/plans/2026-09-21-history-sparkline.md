# Usage-history sparkline (v1.10) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A persisted per-window usage history for the session and weekly quotas, drawn as a sparkline under each bar in the popover, with an on/off toggle.

**Architecture:** New pure module `history.rs` (record/prune/cap/downsample + JSON load/save) owned by the poll thread; `Snapshot.history` carries ≤ 200 downsampled points per key to the popover; a `show_history` setting and a `History line` check item gate a small SVG `<polyline>` in `QuotaCard`.

**Tech Stack:** Rust (serde_json, std fs), Tauri 2.11, React 19, plain SVG, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-21-history-sparkline-design.md`

## Global Constraints

- Only `session` and `weekly` keys are recorded; scoped keys (`weekly:*`) are ignored.
- `MAX_SAMPLES = 5100` per key, `MIN_GAP_SECS = 60`, `POPOVER_POINTS = 200`; file `history.json` in `app_data_dir`; written only when `record` returned `true`.
- Prune rule: keep `t >= resets_at − period_secs`; samples appended in poll order (vector stays sorted).
- `downsample` keeps the last sample always; even stride otherwise; returns the input unchanged when `len <= max`.
- `Snapshot.history: HashMap<String, Vec<Sample>>` (serde, default empty), kept across errors like `quotas`.
- Setting `show_history` default `true`, `KEYS` → 13 (after `show_threshold_marks`), no partner; `PopoverSettings.show_history`; Popover ▸ **History line**.
- SVG: `viewBox="0 0 100 28"`, `preserveAspectRatio="none"`, dotted reference line (0,28)→(100,0), polyline from `sparkPoints(samples, windowStart, period, 100, 28)`; hidden with < 2 samples.
- No network, no token in `history.json`. No `unwrap`/`expect` outside tests. `pnpm verify` green before each commit.
- Commit trailer `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` as a second `-m`. Branch `feat/history-sparkline` (exists, holds the spec).

---

### Task 1: `history.rs` — record, prune, cap, downsample, load/save

**Files:**
- Create: `src-tauri/src/history.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod history;` between `pub mod alerts;` and `pub mod icon;`)

**Interfaces:**
- Consumes: `crate::usage::Quota { key, label, percent, resets_at, period_secs }`.
- Produces: `pub struct Sample { pub t: i64, pub pct: f32 }`, `pub struct History { pub by_key: HashMap<String, Vec<Sample>> }` with `record(&mut self, quotas: &[Quota], now: i64) -> bool` and `for_popover(&self) -> HashMap<String, Vec<Sample>>`; `pub fn downsample(samples: &[Sample], max: usize) -> Vec<Sample>`; `pub fn path(app: &AppHandle) -> Option<PathBuf>`; `pub fn load(path: &Path) -> History`; `pub fn save(path: &Path, h: &History) -> std::io::Result<()>`; constants `MAX_SAMPLES`, `MIN_GAP_SECS`, `POPOVER_POINTS`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/history.rs`:

```rust
//! Per-window usage history for the sparkline: timestamps and percentages only.

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
                Sample { t: NOW - 20_000, pct: 90.0 },
                Sample { t: NOW - 10_000, pct: 10.0 },
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
            .map(|i| Sample { t: window_start + i, pct: 1.0 })
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
}
```

Add `pub mod history;` to `src-tauri/src/lib.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml history::`
Expected: compile errors — `History`, `Sample`, `downsample`, `load`, `save`, constants not found.

- [ ] **Step 3: Implement**

Insert above the `#[cfg(test)]` block:

```rust
use crate::usage::Quota;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub const MAX_SAMPLES: usize = 5100;
pub const MIN_GAP_SECS: i64 = 60;
pub const POPOVER_POINTS: usize = 200;
const FILE_NAME: &str = "history.json";

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
        for q in quotas.iter().filter(|q| q.key == "session" || q.key == "weekly") {
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
    if samples.len() <= max || max == 0 {
        return samples.to_vec();
    }
    let last = samples.len() - 1;
    (0..max)
        .map(|i| samples[i * last / (max - 1)])
        .collect()
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
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec(h).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}
```

`downsample` index math: `i * last / (max − 1)` maps `i = 0` to `0` and `i = max − 1` to `last`, so both ends are kept.

- [ ] **Step 4: Run the tests and verify**

Run: `cargo test --manifest-path src-tauri/Cargo.toml history::` — 7 passed.
Run: `pnpm verify` — green.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/history.rs src-tauri/src/lib.rs
git commit -m "feat: per-window usage history with persistence and downsampling" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: Setting, menu item, snapshot field, poll wiring

**Files:**
- Modify: `src-tauri/src/settings.rs` (`Settings`, `Default`, `KEYS`, `get`, `toggle`, `PopoverSettings`, `From`, tests)
- Modify: `src-tauri/src/tray.rs` (`POPOVER_LABELS`)
- Modify: `src-tauri/src/poll.rs` (`Snapshot`, `Default`, `run`, success branch, test fixtures)
- Modify: `src-tauri/src/main.rs` (`poll::run` call)
- Modify: test fixtures that construct `Snapshot { … }` literally: `src-tauri/src/icon.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/poll.rs` (add `history: HashMap::new()` / `Default::default()`)

**Interfaces:**
- Consumes: `history::{History, Sample, load, save, path}` from Task 1.
- Produces: `Settings.show_history`, `PopoverSettings.show_history`, `Snapshot.history: HashMap<String, Sample>`-valued map (`HashMap<String, Vec<history::Sample>>`), `poll::run(shared, settings, log_path, history_path: Option<PathBuf>, on_update)`.

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/settings.rs` tests: change both `assert_eq!(KEYS.len(), 12);` to `13`, and in the test that asserts `s.auto_update_check` add:

```rust
        assert!(s.show_history);
        assert!(s.toggle("show_history"));
        assert!(!s.get("show_history"));
        assert!(s.toggle("show_history"));
        assert!(PopoverSettings::from(&s).show_history);
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml settings::` — fails (`show_history` missing).

- [ ] **Step 2: Setting + menu label**

`settings.rs`: field `pub show_history: bool,` after `show_threshold_marks`; `Default` sets `show_history: true`; `KEYS: [&str; 13]` with `"show_history"` appended after `"show_threshold_marks"`; `get` arm `"show_history" => self.show_history,`; `toggle` allow-list gains `| "show_history"` next to the other partner-less keys and the flip arm `"show_history" => self.show_history = !self.show_history,`; `PopoverSettings` gains `pub show_history: bool,` and `From` copies it.

`tray.rs`: `POPOVER_LABELS: [(&str, &str); 4]` with `("show_history", "History line")` appended.

- [ ] **Step 3: Snapshot field and poll wiring**

`poll.rs`:
- `use crate::history;` and `use std::collections::HashMap;`.
- `Snapshot` gains, after `extra`: `/// Downsampled per-window samples for the popover sparkline.` `pub history: HashMap<String, Vec<history::Sample>>,`; `Default` sets `history: HashMap::new()`.
- `run` signature: `pub fn run<F>(shared: Shared, settings: Arc<Mutex<Settings>>, log_path: Option<PathBuf>, history_path: Option<PathBuf>, on_update: F)`; at thread start: `let mut history = history_path.as_deref().map(history::load).unwrap_or_default();`.
- In the success branch, right before `let mut s = lock(&shared);` (the one that sets `s.quotas = quotas;`), add:

```rust
                                if history.record(&quotas, now) {
                                    if let Some(path) = &history_path {
                                        // In-memory history is authoritative; a failed write only
                                        // loses persistence across restarts.
                                        let _ = history::save(path, &history);
                                    }
                                }
                                let popover_history = history.for_popover();
```

and after `s.quotas = quotas;` add `s.history = popover_history;`.

`main.rs`: `poll::run(shared, settings, log::path(app.handle()), history::path(app.handle()), move |snapshot| { … })` and add `history` to the `use claude_usage_monitor::{…}` list.

Fixtures: every literal `Snapshot { … }` in tests (`icon.rs` `snapshot()`, `tray.rs` `snapshot()`, `poll.rs` `log_line` test) gains `history: HashMap::new(),` (import `std::collections::HashMap` in those test modules if missing, or write `Default::default()`).

- [ ] **Step 4: Verify**

Run: `pnpm verify` — green (settings tests 13 keys; all fixtures compile).
Run `pnpm tauri dev` ~40 s, then kill it: `~/Library/Application Support/com.matteo.claude-usage-monitor/history.json` exists and contains `"session"` after the first poll (if a token is present). Right-click → Popover shows **History line** checked.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/settings.rs src-tauri/src/tray.rs src-tauri/src/poll.rs src-tauri/src/main.rs src-tauri/src/icon.rs
git commit -m "feat: record usage history per poll and expose it to the popover" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: Sparkline in the popover

**Files:**
- Modify: `src/lib/quota.ts` (`Sample`, `Snapshot.history`, `PopoverSettings.show_history`, default)
- Modify: `src/lib/format.ts` (`sparkPoints`)
- Modify: `test/format.test.ts`
- Modify: `src/lib/ipc.ts` (fixture history)
- Modify: `src/App.tsx` (`QuotaCard`)
- Modify: `src/app.css`

**Interfaces:**
- Consumes: `Snapshot.history` and `PopoverSettings.show_history` (Task 2 serde names).
- Produces: `sparkPoints(samples: Sample[], windowStart: number, period: number, w: number, h: number): string`.

- [ ] **Step 1: Write the failing test**

Append to `test/format.test.ts` (import `sparkPoints` from `@/lib/format`):

```ts
describe('sparkPoints', () => {
  it('maps the window to the box and clamps', () => {
    const start = 1000;
    expect(
      sparkPoints(
        [
          { t: start, pct: 0 },
          { t: start + 500, pct: 150 },
          { t: start + 1000, pct: 100 },
        ],
        start,
        1000,
        100,
        28,
      ),
    ).toBe('0,28 50,0 100,0');
    expect(sparkPoints([{ t: start, pct: 10 }], start, 1000, 100, 28)).toBe('');
  });
});
```

Run: `pnpm test:run` — fails (`sparkPoints` not exported).

- [ ] **Step 2: Implement**

`src/lib/quota.ts`:

```ts
export type Sample = { t: number; pct: number };
```

`Snapshot` gains `history: Record<string, Sample[]>;`; `PopoverSettings` gains `show_history: boolean;`; `DEFAULT_POPOVER_SETTINGS` gains `show_history: true`.

`src/lib/format.ts` (import `Sample` from `./quota`):

```ts
// SVG polyline points for a sparkline over one reset window. Empty below two samples.
export function sparkPoints(
  samples: Sample[],
  windowStart: number,
  period: number,
  w: number,
  h: number,
): string {
  if (samples.length < 2 || period <= 0) return '';
  return samples
    .map((s) => {
      const x = Math.min(w, Math.max(0, ((s.t - windowStart) / period) * w));
      const y = h - (Math.min(100, Math.max(0, s.pct)) / 100) * h;
      return `${round(x)},${round(y)}`;
    })
    .join(' ');
}

function round(n: number): number {
  return Math.round(n * 10) / 10;
}
```

`src/lib/ipc.ts` fixture: add to `FIXTURE` a `history` field:

```ts
  history: {
    session: Array.from({ length: 30 }, (_, i) => ({
      t: nowSecs - 2 * 3600 + i * 240,
      pct: Math.min(48, i * 1.7),
    })),
    weekly: Array.from({ length: 40 }, (_, i) => ({
      t: weeklyReset - WEEKLY_SECS + i * 8640,
      pct: Math.min(64, i * 1.8),
    })),
  },
```

`src/App.tsx` `QuotaCard`: add a `history: Sample[]` prop (caller passes `snap.history[q.key] ?? []`), import `Sample` and `sparkPoints`, and between the `</div>` closing `.bar` and `<p className="reset">` insert:

```tsx
      {settings.show_history && history.length >= 2 && (
        <svg className="spark" viewBox="0 0 100 28" preserveAspectRatio="none" aria-hidden="true">
          <title>Usage over this window</title>
          <line className="pace" x1="0" y1="28" x2="100" y2="0" />
          <polyline
            points={sparkPoints(history, q.resets_at - q.period_secs, q.period_secs, 100, 28)}
          />
        </svg>
      )}
```

`src/app.css`:

```css
.spark {
  display: block;
  width: 100%;
  height: 28px;
  margin-top: 6px;
  overflow: visible;
}

.spark polyline {
  fill: none;
  stroke: var(--ok);
  stroke-width: 1.5;
  vector-effect: non-scaling-stroke;
}

.card.warn .spark polyline {
  stroke: var(--warn);
}

.card.over .spark polyline {
  stroke: var(--over);
}

.spark .pace {
  stroke: var(--muted);
  stroke-width: 1;
  stroke-dasharray: 2 3;
  vector-effect: non-scaling-stroke;
}
```

The card colour classes only recolour `.fill` (`.card.warn .fill`, `.card.over .fill`), so the polyline gets the same three rules; no new colour token.

- [ ] **Step 3: Verify**

Run: `pnpm check:fix`, then `pnpm verify` — green (16 vitest).
Run: `pnpm dev` briefly (the browser fixture) is optional; `pnpm tauri build --bundles app` must succeed.

- [ ] **Step 4: Commit**

```bash
git add src/lib/quota.ts src/lib/format.ts src/lib/ipc.ts src/App.tsx src/app.css test/format.test.ts
git commit -m "feat: sparkline of the current window under each quota bar" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: Docs, changeset, PR

**Files:**
- Modify: `README.md`, `CLAUDE.md`
- Create: `.changeset/history-sparkline.md`

- [ ] **Step 1: README**

Popover overlays list (after the Threshold marks bullet):

```markdown
- **History line** — a small graph under the bar: how usage grew over the current reset window,
  with a dotted diagonal for "on pace". The history is kept in `history.json` next to
  `settings.json`, survives restarts and clears itself at each reset.
```

Configure table, Popover row: `Time ticks · Elapsed marker · Threshold marks · History line`.

- [ ] **Step 2: CLAUDE.md**

Layout block: add `  src/history.rs          per-window usage samples for the sparkline (history.json)` after the `src/update.rs` line. Status line → `**Status:** v1.10 (usage-history sparkline) implemented; released via the Changesets pipeline.` and add the spec path to the Specs list.

- [ ] **Step 3: Changeset, verify, PR**

`.changeset/history-sparkline.md`:

```markdown
---
"claude-usage-monitor": minor
---

Sparkline under each quota bar showing how usage grew over the current reset window (persisted in history.json, cleared at reset); Popover → "History line" toggles it.
```

Run `pnpm verify`, commit (`docs: history sparkline documentation and changeset`), `git push -u origin feat/history-sparkline`, then:

```bash
gh pr create --base main --title "feat: usage-history sparkline" --body "$(cat <<'EOF'
## Summary
- `history.rs`: per-window samples for session/weekly (one per 60 s, cap 4000, pruned before the window start so a reset clears the line), `history.json` in app_data_dir written only when something changed, even-stride downsampling to 200 points
- Poll thread records after each successful fetch; `Snapshot.history` carries the downsampled points
- Popover: SVG sparkline under each bar with a dotted on-pace diagonal; `show_history` setting + Popover ▸ **History line**
- README/CLAUDE.md, minor changeset

## Test plan
- [x] `pnpm verify` green (Rust history tests + settings 13 keys; vitest `sparkPoints`); `.app` builds
- [ ] Installed: line appears after two polls and grows; relaunch keeps it; after a session reset it restarts; toggle hides it live

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Report the PR URL.
