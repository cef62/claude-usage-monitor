# Usage-history sparkline (v1.10) — Design

Date: 2026-09-21. Status: approved.

## Goal

Show how usage grew over the current reset window: a small line under the session and weekly
bars, drawn from the percentages the app already polls, persisted so it survives restarts and
updates, and cleared automatically when the window resets. A dotted diagonal shows the "on
pace" line so the user sees at a glance whether they are ahead of the clock.

## Decisions

| Topic | Decision |
|---|---|
| Scope | `session` and `weekly` quotas only; current window only; no export |
| Samples | `(t, pct)` per successful poll, at most one per 60 s per key, capped at 4000 per key |
| Pruning | on every record, samples with `t < resets_at − period_secs` are dropped — the reset empties the line by itself, no reset detection needed |
| Persistence | `history.json` in `app_data_dir`, written only when a record changed something; corrupt/missing → empty |
| Transport | `Snapshot.history: HashMap<String, Vec<Sample>>`, downsampled to ≤ 200 points per key (even stride, last sample always kept) |
| Rendering | SVG polyline 28 px tall under the bar, x = `(t − window_start) / period`, y = `pct` clamped 0..100, stroke = the bar's colour; dotted diagonal (0,0)→(100 %,100 %) reference; hidden with < 2 samples |
| Setting | `show_history: bool` (default true), `KEYS` → 13, Popover ▸ **History line**, `PopoverSettings.show_history` |
| Network | none; no cadence change |

## Out of scope

Multi-window history, CSV export, per-model quotas, tray/icon changes, tooltips on the line.

## Components

### `history.rs` (new)

```rust
pub const MAX_SAMPLES: usize = 4000;
pub const MIN_GAP_SECS: i64 = 60;
pub const POPOVER_POINTS: usize = 200;
const FILE_NAME: &str = "history.json";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sample { pub t: i64, pub pct: f32 }

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct History { pub by_key: HashMap<String, Vec<Sample>> }

impl History {
    /// Records `session`/`weekly` quotas at `now`; prunes anything before each quota's window
    /// start; skips a key whose last sample is younger than MIN_GAP_SECS; caps at MAX_SAMPLES.
    /// Returns true when anything changed (caller saves only then).
    pub fn record(&mut self, quotas: &[Quota], now: i64) -> bool;
    /// ≤ POPOVER_POINTS per key, even stride, last sample always present.
    pub fn for_popover(&self) -> HashMap<String, Vec<Sample>>;
}

pub fn downsample(samples: &[Sample], max: usize) -> Vec<Sample>;
pub fn path(app: &AppHandle) -> Option<PathBuf>;   // app_data_dir/history.json
pub fn load(path: &Path) -> History;                // empty on any error
pub fn save(path: &Path, h: &History) -> std::io::Result<()>;
```

`record` prune rule per quota: `window_start = q.resets_at − q.period_secs as i64`; keep
`t >= window_start`. Samples are appended in poll order, so the vector stays sorted.

### `poll.rs`

- `run(shared, settings, log_path, history_path: Option<PathBuf>, on_update)`; loads
  `History` once at thread start.
- After a successful usage fetch (the branch that sets `Status::Ok`): `if history.record(&quotas, now) { save }` (errors ignored — the in-memory copy is authoritative), then
  `s.history = history.for_popover()`.
- `Snapshot.history: HashMap<String, Vec<history::Sample>>`, default empty, kept across errors.

### `settings.rs` / `tray.rs`

- `show_history: bool` default `true`; `KEYS` 13; `get`/`toggle` arms (no partner);
  `PopoverSettings.show_history`.
- `POPOVER_LABELS` gains `("show_history", "History line")` (4 entries).

### Frontend

- `quota.ts`: `Sample = { t: number; pct: number }`, `Snapshot.history: Record<string, Sample[]>`,
  `PopoverSettings.show_history` (+ default true).
- `format.ts`: `sparkPoints(samples, windowStart, period, w, h): string` — SVG `points`
  attribute, x = `((t − windowStart) / period) * w` clamped 0..w, y = `h − (pct / 100) * h`
  clamped; empty string when < 2 samples.
- `App.tsx` `QuotaCard`: when `settings.show_history` and the key has ≥ 2 samples, render
  `<svg className="spark" viewBox="0 0 100 28" preserveAspectRatio="none">` with a dotted
  `<line x1=0 y1=28 x2=100 y2=0>` and `<polyline points=…>`; placed between the bar and the
  reset line. CSS: `.spark { width: 100%; height: 28px; }`, polyline stroke `currentColor`
  1.5 px (card colour already sets `color`), line stroke `var(--muted)` dashed.
- `ipc.ts` fixture: 30 samples over the last 2 h for `session`, 40 over 4 d for `weekly`.

### Docs and release

README Popover overlay list gains **History line**; Configure table Popover row; changeset
`minor`. CLAUDE.md layout gains `src/history.rs`.

## Testing

Rust:
- `record`: first call appends one sample per session/weekly quota and ignores `weekly:fable`;
  a second call 30 s later appends nothing (returns false); 60 s later appends; a sample older
  than the window start is dropped when the next record runs; 4001 samples → 4000 (oldest gone).
- `downsample`: 1000 → 200 with the last sample kept; 50 → 50 unchanged.
- `load`/`save` round-trip in a temp dir; `load` of garbage → empty.
- `settings`: `KEYS.len() == 13`, `show_history` default true and toggles.

TypeScript: `sparkPoints` with two samples at window start and end → `"0,28 100,0"`; single sample
→ `""`; pct 150 clamps to y 0.

Manual: line grows across polls; relaunch keeps it; after a session reset the line restarts;
Popover ▸ History line hides it live.

## Security notes

No network, no token; `history.json` holds timestamps and percentages only.
