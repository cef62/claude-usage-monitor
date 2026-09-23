# Pace forecast — Design

Date: 2026-09-22. Status: approved.

## Goal

Tell the user whether they will run out before the window resets, at the pace they are using
Claude right now. One line per session/weekly card in the popover ("At this pace: 100% at 16:40"
or "At this pace: ~78% at reset"), and one notification per window when a run-out is projected
early enough to matter. Built entirely from the history samples the app already records; no new
network traffic.

The elapsed-time marker already shows the window-average pace. The forecast adds the *recent*
rate: after a burst it warns sooner, after going idle it stops predicting a run-out that will not
happen.

## Decisions

| Topic | Decision |
|---|---|
| Scope | `session` and `weekly` quotas only (the keys `history.rs` records); per-model weekly quotas have no history and get no forecast |
| Rate | trailing rate over a lookback of `period_secs / 7`: 2571 s (~43 min) for the session, 86 400 s (24 h) for the weekly quota |
| Base point | newest sample with `t <= now − lookback`, using its real `t`; none → window start at 0 % (windows always start at 0 %, so this is the window average) |
| Too early | no forecast while the window is younger than `lookback / 2` (~21 min session, 12 h weekly) |
| Result | `RunsOut { at }` when 100 % comes by `resets_at` (`<=`), else `AtReset { percent }` (always < 100); no forecast at ≥ 100 %. `RunsOut` also carries `from_average` (not serialized) when the base was the window-start fallback |
| Where computed | Rust only (`forecast.rs`), from the full-resolution samples; shipped in `Snapshot.forecast`. Popover and notification read the same value |
| Popover | one line under "Resets in …"; red when `runs_out`, muted otherwise; hidden while the snapshot is stale, once the run-out time or the reset has passed, and when Popover ▸ **Pace forecast** is off; "at reset" shows at most ~99 % |
| Notification | checked once per successful poll (`fetched_at`), not on every title tick; once per quota per window, when `RunsOut` from a real trend (not `from_average`) and the run-out is at least one lookback before the reset and usage is below the quota's top alert level; merged into a threshold alert that fires for the same quota in the same poll |
| Settings | `alert_forecast: bool` (default true), Alerts ▸ **Run-out forecast**, gated by the quota's Session/Weekly alert switch like the other alerts; `show_forecast: bool` (default true), Popover ▸ **Pace forecast**, in `PopoverSettings`; `KEYS` → 15 |
| Network | none; no cadence change |

## Out of scope

Menu bar title / Windows icon changes, per-model quotas, a second
notification when the pace picks up again in the same window, local clock times in notifications
(Rust has no time-zone data; notifications use relative times like the existing ones), persisting
the notification state across restarts.

## Components

### `forecast.rs` (new, pure)

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Forecast {
    /// 100 % is reached before the reset, at this unix second.
    RunsOut { at: i64 },
    /// Projected utilization when the window resets; always < 100.
    AtReset { percent: f64 },
}

/// `period_secs / 7`: ~43 min for the session, exactly one day for the weekly quota.
pub fn lookback(period_secs: u64) -> i64;

pub fn forecast(q: &Quota, samples: &[Sample], now: i64) -> Option<Forecast>;

/// `forecast` for each `session`/`weekly` quota, keyed by quota key; quotas without a forecast
/// are absent.
pub fn for_quotas(quotas: &[Quota], history: &History, now: i64) -> HashMap<String, Forecast>;
```

`forecast` steps:

1. `None` when `q.percent >= 100`, when `now >= q.resets_at`, or when
   `now − window_start < lookback / 2`, where `window_start = q.resets_at − period_secs`.
2. Base `(t0, p0)` = the newest sample with `window_start <= t <= now − lookback`; if none,
   `(window_start, 0.0)`.
3. `rate = (q.percent − p0) / (now − t0)` in % per second (`None` if `now − t0 <= 0`).
4. `rate <= 0` → `AtReset { percent: q.percent }`.
5. `run_out = now + ceil((100 − q.percent) / rate)`. `run_out < q.resets_at` →
   `RunsOut { at: run_out }`, else `AtReset { percent: q.percent + rate × (q.resets_at − now) }`.

### `poll.rs`

- In the success branch, after `history.record(...)` and before `quotas` moves into the
  snapshot: `let forecasts = forecast::for_quotas(&quotas, &history, now);` then
  `s.forecast = forecasts;`.
- `Snapshot.forecast: HashMap<String, forecast::Forecast>`, default empty, kept across errors
  like `quotas`.

### `alerts.rs`

```rust
pub struct RunOut { pub key: String, pub label: String, pub at: i64, pub resets_at: i64 }

pub fn run_outs(
    state: &mut AlertState,
    quotas: &[Quota],
    forecasts: &HashMap<String, Forecast>,
    settings: &Settings,
    now: i64,
) -> Vec<RunOut>;
```

- `AlertState` gains `forecast_fired: HashSet<(String, i64)>` (quota key, `resets_at`), pruned
  with the same `PRUNE_AFTER_SECS` rule as `fired`; "already fired" compares `resets_at` within
  `SAME_WINDOW_SECS`, like the threshold alerts.
- A quota is due when all hold: `settings.alert_forecast`; `enabled(settings, key)`; its forecast
  is `RunsOut { at }`; `q.resets_at − at >= forecast::lookback(q.period_secs)`; `q.percent` is
  below `top(settings.levels(key))` (or no levels are configured); not already fired for this
  window. Every returned `RunOut` is recorded in `forecast_fired`, whether `main.rs` sends it
  alone or merges it into a threshold alert.

### `main.rs`

`notify_thresholds` calls `alerts::run_outs` after `alerts::evaluate`, with `snapshot.forecast`.
For each `RunOut`:

- a threshold alert for the same key is due in this cycle → no separate notification; that
  alert's body becomes `Resets in {countdown} · at this pace 100% in ~{countdown}`;
- otherwise a notification titled `Claude usage: {label}` with body
  `At this pace: 100% in ~{countdown(at − now)} · resets in {countdown(resets_at − now)}`.

Each run-out is logged as `forecast {key} 100% in {countdown}`.

### `settings.rs` / `tray.rs`

- `alert_forecast: bool` default `true`; `KEYS` 14; `get`/`toggle` arms (no partner; the
  `toggle` doc comment lists it with the other free keys). Older files get the default through
  `#[serde(default)]`. Not part of `PopoverSettings`.
- `ALERT_LABELS` gains `("alert_forecast", "Run-out forecast")` after "Notify on reset"
  (4 entries).

### Frontend

- `quota.ts`: `Forecast = { kind: 'runs_out'; at: number } | { kind: 'at_reset'; percent: number }`;
  `Snapshot.forecast: Record<string, Forecast>`.
- `format.ts`: `forecastText(f: Forecast, now: number): string | null` —
  `runs_out` → `At this pace: 100% at ${clock(f.at, now)}` (weekday prefix when not today);
  `at_reset` → `At this pace: ~${Math.round(f.percent)}% at reset`. Returns null for a
  `runs_out` whose `at` is not in the future (a snapshot from before a sleep or a long
  backoff); the card then shows no line.
- `App.tsx` `QuotaCard`: new prop `forecast: Forecast | undefined` (from
  `snap.forecast[q.key]`); when present, a `<p>` after the reset line with class `reset`, plus
  `runs-out` for `runs_out`. CSS: `.reset.runs-out { color: var(--over); }`.
- `ipc.ts` fixture: `forecast: { session: { kind: 'runs_out', at: nowSecs + 80 * 60 },
  weekly: { kind: 'at_reset', percent: 78 } }`.

### Docs and release

README: Popover list gains **Pace forecast**; Alerts list gains **Run-out forecast**; Configure
table Alerts row gains `Run-out forecast`. CLAUDE.md: layout gains `src/forecast.rs`, spec list
gains this file, status line updated. Changeset `minor`. History record
`history/2026-09-22-pace-forecast.md`.

## Testing

Rust, `forecast.rs` (session quota, `period = 18000`, `resets_at = NOW + 14400`, so
`window_start = NOW − 3600`):
Float results are compared with a tolerance (±1 s for `at`, ±0.01 for `percent`).

- steady rate: samples `(NOW − 3000, 20)`, current 50 → rate 0.01 %/s →
  `RunsOut { at: NOW + 5000 }`;
- slow rate: same base, current 30 → `AtReset { percent: 78.0 }`;
- flat: base 30, current 30 → `AtReset { percent: 30.0 }`;
- base selection: a newer sample inside the lookback (`NOW − 1000`) is ignored; the newest sample
  at or before `NOW − 2571` is used with its own `t`;
- no old-enough sample → window start at 0 %: current 50 at 3600 s elapsed →
  `RunsOut { at: NOW + 3600 }`;
- too early: window 1000 s old → `None`; `percent >= 100` → `None`; `now >= resets_at` → `None`;
- weekly quota: `lookback(604800) == 86400`;
- `for_quotas` skips `weekly:fable`.

Rust, `alerts::run_outs`:
- fires once per window and again after the reset; jittered `resets_at` (±0.5 s) is the same
  window;
- silent when `alert_forecast` is off, when `alert_session` is off, when the run-out is less
  than one lookback before the reset, and at or above the top level;
- `forecast_fired` entries older than `PRUNE_AFTER_SECS` are pruned.

Rust, `settings.rs`: `KEYS.len() == 14`; `alert_forecast` default true, toggles freely, old file
without it loads as true. `poll.rs` / `tray.rs`: existing fixtures gain the new field / item.

TypeScript, `forecastText`: `runs_out` today → `At this pace: 100% at HH:MM`; `runs_out`
tomorrow → weekday prefix; `at_reset` 77.6 → `At this pace: ~78% at reset`.

Manual: `pnpm dev` shows both fixture lines; `pnpm tauri dev` shows real forecasts; Alerts ▸
Run-out forecast toggles live; a burst of usage early in a session produces one notification.

## Revisions after PR review (2026-09-23)

The maintainer's review of PR #43 changed these points. The Decisions table includes them; the
Components and Testing sections describe the design as first approved.

- `run_outs` ran on every title tick against the cached snapshot, so with polls stalled a toggle
  switched on later could announce a frozen (even past) run-out. It now runs once per
  `fetched_at`.
- A window-average forecast (no sample a full lookback old) could spend the window's only alert
  on an early burst. `RunsOut.from_average` keeps it in the popover and out of the alerts.
- Reaching 100 % exactly at the reset gave `AtReset { percent: 100 }`; it is now `RunsOut`, and
  the popover caps "at reset" at 99 %.
- An `at_reset` line outlived its window while polls failed, and a stale popover showed
  yesterday's pace: `forecastText` takes `resets_at`, and the card hides the line while stale.
- Popover ▸ Pace forecast toggle (`show_forecast`), like every other popover line.
- `history::tracked(key)` is the one session/weekly filter for history and forecast.
- README: all alerts follow the quota's Session/Weekly switch.

## Security notes

No network, no token, nothing new on disk: the forecast is recomputed from `history.json`
samples and the notification state is in memory only.
