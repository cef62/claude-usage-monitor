# Configuration, Bar Overlays and Log (v1.3) — Design

Date: 2026-09-17. Status: approved.

## Goal

Make thresholds, poll interval and popover overlays configurable from the tray menu (persisted in
`settings.json`), draw the configured thresholds on the popover bars, add a "Send test
notification" item, and keep a capped local log with a menu item that reveals it in Finder.

## Decisions

| Topic | Decision |
|---|---|
| Threshold presets | Session `80/95` (default) · `50/80/95` · `90/95`; Weekly `95` (default) · `80/95` · `90`. Radio submenus. Any values hand-editable in `settings.json` |
| Marker / time-aware rule | `top = max(levels)` per quota: `⚠` and always-fire at `top`; lower levels are time-aware |
| Poll interval | Presets 3 / 5 / 10 / 15 min (radio); stored `poll_interval_secs`, clamped 120..900; cooldown and 429 backoff unchanged |
| Popover overlays | Three toggles: time ticks, elapsed marker, threshold marks |
| Settings state | `Arc<Mutex<Settings>>` managed; poll thread reads it each cycle; `Settings` is `Clone`, not `Copy` |
| Log | `app_data_dir/claude-usage-monitor.log`, rotate at 1 MB to `.log.1`, never the token |
| Open log | `tauri_plugin_opener::reveal_item_in_dir` from Rust (no capability) |

## Out of scope

Numeric editors in the UI, Windows tray, notification click handling, per-model alerts, log
upload/sharing beyond revealing the file.

## Components

### `settings.rs`

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // existing 7 bools …
    pub show_time_ticks: bool,        // true
    pub show_elapsed_marker: bool,    // true
    pub show_threshold_marks: bool,   // true
    pub session_levels: Vec<u8>,      // [80, 95]
    pub weekly_levels: Vec<u8>,       // [95]
    pub poll_interval_secs: u64,      // 180
}
pub const KEYS: [&str; 10]  // + "show_time_ticks", "show_elapsed_marker", "show_threshold_marks"
pub const MIN_POLL_SECS: u64 = 120;
pub const MAX_POLL_SECS: u64 = 900;
pub const SESSION_LEVEL_PRESETS: [&[u8]; 3] = [&[80, 95], &[50, 80, 95], &[90, 95]];
pub const WEEKLY_LEVEL_PRESETS: [&[u8]; 3] = [&[95], &[80, 95], &[90]];
pub const INTERVAL_PRESETS: [u64; 4] = [180, 300, 600, 900];

impl Settings {
    pub fn levels(&self, key: &str) -> &[u8];          // "session" | "weekly" | else &[]
    pub fn set_levels(&mut self, key: &str, levels: &[u8]) -> bool;
    pub fn set_poll_interval(&mut self, secs: u64);   // clamps
    pub fn repair(&mut self);  // existing pairs + levels sorted/deduped/1..=100, empty → default; interval clamped
}
pub fn format_levels(levels: &[u8]) -> String;       // "80/95"
pub fn parse_levels(s: &str) -> Option<Vec<u8>>;      // "80,95" → [80, 95], validated
```

`toggle` covers the three new bools (no partner). `load` still calls `repair`.

### `alerts.rs`

- `levels(key)` reads `settings.levels(key)`; `MARKER_LEVEL` constant is replaced by
  `top(levels) = levels.iter().max()`.
- Due rule: `percent >= level`, not fired for `(key, resets_at ± 60 s, level)`, and for
  `level < top` also `percent > elapsed_pct`.
- `marker`: any alert-enabled quota with `percent >= top(levels)`; a quota with no levels never
  marks.

### `poll.rs`

- `run(shared, settings: Arc<Mutex<Settings>>, on_update)`; each cycle reads
  `base = settings.poll_interval_secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS)`.
- `next_delay(outcome, error_count, nearest_reset, now, base)`: Success → `base`, or
  reset-aligned `max(COOLDOWN)` when sooner; 429 backoff uses `base` as its first step and
  `Retry-After` clamp lower bound; other arms unchanged.
- Emits a log line per cycle (see Log).

### `log.rs` (new)

```rust
pub const MAX_BYTES: u64 = 1_048_576;
pub fn path(app: &AppHandle) -> Option<PathBuf>;              // app_data_dir/claude-usage-monitor.log
pub fn append(path: &Path, line: &str) -> io::Result<()>;     // "<RFC3339 UTC> <line>\n", rotates first if > MAX_BYTES
pub fn write(app: &AppHandle, line: &str);                    // path + append, errors ignored
```

Lines: `startup v<version>`, `poll ok session=48% weekly=64% next=180s`, `poll rate_limited
retry=360s`, `poll auth_expired`, `poll no_token`, `poll error <message>`, `alert <key> <level>
at <percent>%`, `settings <key>=<value>`, `test notification`. RFC3339 is produced by a small
hand-written formatter from unix seconds (no `chrono`). Rotation: rename to `<name>.1`
(overwriting) then start a new file.

### Menu (`tray.rs`)

```
Open usage page
Menu bar ▸    Session ✓ · Weekly ✓ · Glyphs ✓ · Percent ✓ · Remaining time ✓
Popover ▸     Time ticks ✓ · Elapsed marker ✓ · Threshold marks ✓
Alerts ▸      Session ✓ · Weekly ✓ · ─ · Session levels ▸ · Weekly levels ▸ · ─ · Send test notification
Check every ▸ 3 min · 5 min · 10 min · 15 min
Help ▸        Open log
─
Quit
```

- Check groups use ids `set:<key>` (existing mechanism, `KEYS` = 10).
- Radio groups use `CheckMenuItem`s with ids `levels:session:<a,b>`, `levels:weekly:<a,b>`,
  `interval:<secs>`; the handler applies the value, then sets exactly the matching item checked
  (none when the stored value matches no preset), saves, logs `settings …`, emits `settings`.
- `test-notification` → shows `Claude usage: test` / `Notifications are working`, logs.
- `open-log` → `tauri_plugin_opener::reveal_item_in_dir(path)`; if the file does not exist yet,
  create it with a `startup` line first.
- Managed `MenuItems` holds every check/radio item (`HashMap<String, CheckMenuItem<Wry>>` keyed
  by id).

### Popover (`src/`)

- New command `get_settings() -> PopoverSettings` and event `settings` (emitted from
  `on_setting_toggled` and radio handlers) with:
  `{ session_levels: number[], weekly_levels: number[], show_time_ticks, show_elapsed_marker, show_threshold_marks }`.
- `ipc.ts`: `getSettings()`, `onSettings(cb)`; fixture `{ [80,95], [95], true, true, true }`.
- `App.tsx`: fetch settings on mount, subscribe; `QuotaCard` receives `overlays` and `levels`
  (session → `session_levels`, weekly → `weekly_levels`, per-model → none). Renders
  `.tick` only when `show_time_ticks`, `.marker` only when `show_elapsed_marker`, and
  `.mark` elements at each level when `show_threshold_marks` — class `mark over` for the top
  level, `mark warn` for the others. CSS: 2 px tall ticks below the bar in amber/red.
- `format.ts`: `markClass(level, levels): 'over' | 'warn'` (pure, tested).

### Delivery changes (`main.rs`)

- Manage `Arc<Mutex<Settings>>` (replaces `Mutex<Settings>`), pass a clone to `poll::run`.
- `notify_thresholds` logs each alert. Startup writes `startup v<tauri.conf version>`.

### Docs and release

- README: "Configuration" section (menu items, `settings.json` fields with ranges, log location,
  test notification). CLAUDE.md: layout gains `src/log.rs`; commands note for the log path.
- Changeset `minor`.

## Testing

Rust:
- `settings`: new fields default; `repair` sorts/dedups levels, drops `0`/`>100`, empty → default,
  clamps interval 50→120 and 5000→900; `parse_levels("80,95")`, `parse_levels("x")` → None,
  `format_levels`; `set_levels`/`set_poll_interval`; `KEYS.len() == 10`; old file loads.
- `alerts`: `90/95` → 90 time-aware, 95 top; `95` alone → 95 fires regardless of clock, 80%
  silent; `50/80/95` at 55% ahead of clock → level 50; marker uses top (`90/95` at 92% → false,
  at 96% → true).
- `poll::next_delay`: base 300 → 300; reset in 60 s with base 300 → `COOLDOWN`; 429 without
  Retry-After, base 600, count 1 → 600, count 3 → 900.
- `log`: `append` writes RFC3339 prefix (`1789588800` → `2026-09-17T20:00:00Z`); rotation when the
  file exceeds `MAX_BYTES` (temp dir, write 1 MB + 1 line → `.1` exists, new file has one line);
  a line never contains a given token string (test passes a fake `sk-ant-…` in the message to
  prove the caller, not the logger, is responsible — documented, not enforced).
- `tray`: radio id round-trip (`levels:session:80,95` → key `session`, levels `[80, 95]`;
  `interval:300` → 300; malformed → None).

TypeScript (`test/format.test.ts`): `markClass(95, [80, 95]) === 'over'`, `markClass(80, [80, 95]) === 'warn'`.

Manual: menus render; Send test notification shows a banner (and the macOS permission prompt the
first time); Help → Open log reveals the file; changing "Check every" changes the footer's
"next Xm" on the following cycle; toggling overlays updates the popover live; presets persist
across relaunch.

## Security notes

No new capabilities. The log never receives the token or any request header; only percentages,
statuses, delays, level numbers and setting values.
