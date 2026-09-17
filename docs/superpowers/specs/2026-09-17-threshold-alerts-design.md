# Threshold Alerts (v1.2) — Design

Date: 2026-09-17. Status: approved.

## Goal

Warn the user before a quota runs out: a macOS notification when the session quota crosses 80%
or 95% and when the weekly quota crosses 95%, once per reset window, with the 80% alert
suppressed while usage is still behind the clock. A `⚠` marker in the menu bar title while any
alerting quota is at or above 95%. Per-quota on/off in the tray menu.

## Decisions

| Topic | Decision |
|---|---|
| Delivery | `tauri-plugin-notification` 2.4 (Rust side only) + `⚠ ` title prefix |
| Levels | Session `[80, 95]`, weekly `[95]`; fixed |
| Configuration | Tray submenu **Alerts** → `Session`, `Weekly` check items; `Settings` gains `alert_session`, `alert_weekly` (default true) |
| Once per window | Keyed by `(quota key, resets_at, level)`; in-memory only |
| Time-aware | Levels below 95 fire only when `percent > elapsed_pct` |
| Click handling | None — the plugin's action API is mobile-only |

## Out of scope

Reset notification, custom thresholds, notification click handling, popover banner for alerts,
per-model quota alerts, persisting alert state across restarts.

## Components

### `settings.rs`

- Two new fields with `#[serde(default)]` semantics via the existing struct-level attribute:
  `alert_session: bool`, `alert_weekly: bool`; `Default` sets both true.
- `KEYS` becomes `["session", "weekly", "glyph", "percent", "remaining", "alert_session", "alert_weekly"]`.
- `get`/`toggle` handle the new keys; no pair invariant (both alerts may be off). `repair()`
  unchanged.

### `alerts.rs` (new, pure)

```rust
pub const SESSION_LEVELS: [u8; 2] = [80, 95];
pub const WEEKLY_LEVELS: [u8; 1] = [95];
pub const MARKER_LEVEL: u8 = 95;

#[derive(Default)]
pub struct AlertState { fired: HashSet<(String, i64, u8)> }   // (key, resets_at, level)

#[derive(Debug, Clone, PartialEq)]
pub struct Alert { pub key: String, pub label: String, pub level: u8, pub percent: f64, pub resets_at: i64 }

pub fn elapsed_pct(q: &Quota, now: i64) -> f64;   // clamp((period − (resets_at − now)) / period × 100, 0, 100)
pub fn enabled(settings: &Settings, key: &str) -> bool;   // "session" → alert_session, "weekly" → alert_weekly, else false
pub fn evaluate(state: &mut AlertState, quotas: &[Quota], settings: &Settings, now: i64) -> Vec<Alert>;
pub fn marker(quotas: &[Quota], settings: &Settings) -> bool;   // any enabled quota with percent >= MARKER_LEVEL
```

`evaluate` rules, per quota with `enabled(settings, key)`:

1. Levels for the key (`session` → `SESSION_LEVELS`, `weekly` → `WEEKLY_LEVELS`), ascending.
2. A level is *due* when `percent >= level` and `(key, resets_at, level)` is not in `fired`, and,
   for `level < MARKER_LEVEL`, `percent > elapsed_pct(q, now)`.
3. Collect due levels; if any, emit ONE `Alert` for the highest due level and insert every
   `level <= highest` into `fired` (so an 80 crossed together with 95 never fires later).
4. Prune `fired` entries whose `resets_at < now − 86400` on every call (bounded memory).

Quotas whose key is not `session`/`weekly` are ignored. `AlertState` lives in a
`Mutex<AlertState>` in Tauri managed state.

### Title marker (`tray.rs`)

`title(s, now, settings)`: for `Ok` and `RateLimited`, if `alerts::marker(&s.quotas, settings)`
the rendered halves are prefixed with `⚠ ` after the leading space: ` ⚠ ◷ 96% ↻1h02m  ·  ▦ 40% ↻3d4h`.
Status strings unaffected. The marker follows the alert settings, not the display settings, so a
hidden half can still raise it.

### Delivery (`main.rs` / `tray.rs`)

- `Cargo.toml`: `tauri-plugin-notification = "2.4"`. `main.rs`: `.plugin(tauri_plugin_notification::init())`.
- No capability entry: the Rust-side plugin call bypasses the webview capability system.
- Poll closure (in `main.rs` `setup`): after `tray::refresh_title`, lock `AlertState`, call
  `evaluate(&mut state, &snapshot.quotas, &settings, now)`, and for each alert show
  `app.notification().builder().title("Claude usage: {label} {percent}%").body("Resets in {countdown}").show()`
  (result ignored). `label` is `Session` / `Weekly`; percent rounded.
- Settings changes take effect on the next poll cycle; no re-evaluation on toggle.

### Menu (`tray.rs`)

Right-click: `Open usage page` · `Menu bar ▸` (existing) · `Alerts ▸` [`Session`, `Weekly`] ·
separator · `Quit`. The `Alerts` items use ids `set:alert_session`, `set:alert_weekly` and the
existing `on_setting_toggled` handler; `MenuItems` holds all seven.

### Docs and release

- README: "Alerts" paragraph (levels, once per window, time-aware rule, how to turn off, first
  notification triggers the macOS permission prompt).
- CLAUDE.md: layout gains `src/alerts.rs`; capabilities note lists `notification:default`.
- Changeset `minor`.

## Testing

Rust, inline `#[cfg(test)]`:

- `alerts::elapsed_pct`: mid-window, before start, after reset.
- `alerts::evaluate` table (session quota, period 18000):
  - 85% with 2h left (elapsed 60%) → one alert, level 80.
  - 30% with 4h left (elapsed 20%) → none (below every level).
  - 82% with 30 min left (elapsed 90%) → none (time-aware: 82 ≤ 90).
  - 96% at any elapsed → one alert level 95; a following call at 97% → none.
  - 96% first call → level 95 only (80 marked fired too); percent drops to 85 later in the same window → none.
  - New `resets_at` (next window) with 85% ahead of clock → fires again.
  - `alert_session` off → none; weekly at 96% with `alert_weekly` on → one weekly alert; weekly at 90% → none.
  - Scoped key `weekly:fable` at 99% → none.
  - Prune: an entry from a window that ended two days ago is dropped after a call.
- `alerts::marker`: session 95 with alerts on → true; alerts off → false; weekly 96 hidden from the
  title (`settings.weekly == false`) but `alert_weekly` on → true.
- `tray::title`: `⚠` prefix cases for `Ok` and `RateLimited`; none for status strings.
- `settings`: new keys in `KEYS`, `toggle` of both alert keys to off allowed, file with old fields loads with alerts on.
- Manual: run the bundled app, lower `SESSION_LEVELS` temporarily is NOT done; instead verify via
  `tauri dev` that no panic occurs and, if a real quota is ≥ 80%, a banner appears once.

## Security notes

No new capabilities. No network, no IPC changes.
