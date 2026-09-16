# Claude Usage Monitor v1 — Design

Date: 2026-09-16. Status: approved. Plan: `docs/superpowers/plans/2026-09-16-usage-monitor-v1.md`.

## Goal

A macOS menu bar app that shows Claude plan usage at a glance and opens a popover with the same
data presented better, plus links to the claude.ai usage and billing pages. Windows tray support
comes later; the code must keep compiling there but ships nothing Windows-specific in v1.

## Decisions

| Topic | Decision |
|---|---|
| Menu bar text | `⏱ 48% ↻2h13m · 📅 64% ↻3d4h` — session and weekly percent with countdown to reset, unicode glyphs, no icon assets beyond a template glyph |
| Popover | Anchored under the menu bar item, no title bar, closes on focus loss or Esc |
| Popover content | Usage bars with elapsed-time marker, links row, status footer. No plan/account line |
| Notifications | Not in v1 |
| Presence | Menu bar only (`LSUIElement`), no Dock icon, no launch-at-login |
| Architecture | Rust owns credentials, polling, tray title. React renders the popover from an emitted snapshot |

## Out of scope for v1

Threshold notifications, settings UI, profile/plan display, usage history and sparkline, Windows
tray icon rendering, launch at login, code signing, auto-update, idle detection.

## Architecture

```
src-tauri/src/
  main.rs     Tauri builder, managed state, commands, activation policy
  lib.rs      module declarations (usage, poll, tray) so tests and main share them
  usage.rs    credentials, HTTP fetch, normalization into Quota
  poll.rs     poll loop thread, delay policy, 401 latch, snapshot state
  tray.rs     tray icon, menu, title formatting, popover positioning
src/
  main.tsx, App.tsx, app.css
  lib/ipc.ts      the only file that calls invoke()/listen(); browser fixture when not in Tauri
  lib/format.ts   pure formatting: countdown, clock, elapsedPct, barColor, relative
  lib/quota.ts    Snapshot/Quota/Status types (mirror of Rust, snake_case)
test/format.test.ts
```

Data flow: poll thread → `Mutex<Snapshot>` → `app.emit("usage", snapshot)` and tray title. The
popover calls `get_snapshot` on mount and subscribes to `usage` events. The frontend never sees
the OAuth token.

## Rust data layer

### Credentials (`usage.rs`)

`read_credentials() -> Option<Credentials { token: String, fingerprint: u64 }>`

- macOS: run `security find-generic-password -s "Claude Code-credentials" -w` with
  `std::process::Command`, parse stdout as JSON. No keychain crate.
- Other OS: read `$CLAUDE_CONFIG_DIR/.credentials.json` (default `~/.claude/.credentials.json`).
- Token path: `claudeAiOauth.accessToken`. Any failure (command error, malformed JSON, missing
  key, empty string) returns `None`.
- `fingerprint` is a hash of the raw blob so the 401 latch can detect a change without keeping the
  token around. Called on every poll cycle.

### Fetch (`usage.rs`)

`fetch_usage(client, base_url, token, user_agent) -> Result<serde_json::Value, FetchError>`

- `reqwest` blocking client, 10s timeout, rustls with native root certificates.
- Headers: `Authorization: Bearer <token>`, `anthropic-beta: oauth-2025-04-20`,
  `User-Agent: <user_agent>`, `Content-Type: application/json`.
- `user_agent` is computed once at startup: `claude-code/<version>` where version comes from
  `claude --version` (first token of stdout), falling back to the constant `2.1.273`.
- `base_url` is a parameter so a manual run against the real API stays possible; production value
  is `https://api.anthropic.com`.

```rust
enum FetchError {
    Unauthorized,                              // 401
    RateLimited { retry_after: Option<u64> },  // 429, seconds from Retry-After if parseable
    Server(u16),                               // any other non-2xx
    Network(String),                           // transport, TLS, timeout, JSON parse
}
```

### Normalize (`usage.rs`)

`normalize(&Value) -> Vec<Quota>`

```rust
struct Quota {
    key: String,       // "session" | "weekly" | "weekly:<model display name lowercased>"
    label: String,     // "Session" | "Weekly" | "<model> weekly"
    percent: f64,      // raw utilization, may exceed 100; UI clamps for drawing only
    resets_at: i64,    // unix seconds
    period_secs: u64,  // 5 * 3600 for session, 7 * 86400 for weekly kinds
}
```

Rules:

1. If `limits` is an array, take entries with `kind` in `session`, `weekly_all`, `weekly_scoped`.
   `weekly_scoped` uses `scope.model.display_name` for the key and label.
2. Otherwise fall back to flat fields `five_hour` → session, `seven_day` → weekly,
   `seven_day_<name>` → `weekly:<name>` for any object-valued field with that prefix.
3. Drop any entry whose percent (`percent` in `limits[]`, `utilization` in flat fields) is null
   or whose `resets_at` is missing or unparseable.
4. Order: session, weekly, then scoped alphabetically.

### Snapshot and status

```rust
struct Snapshot {
    quotas: Vec<Quota>,          // last good values; never emptied by an error
    fetched_at: Option<i64>,     // unix seconds of last successful fetch
    next_poll_at: i64,           // unix seconds; UI uses it for "next in Xm" and stale dimming
    status: Status,
}

enum Status {
    Ok,
    NoToken,
    AuthExpired,
    RateLimited { until: i64 },
    Error { message: String },   // short, user-readable; never contains the token
}
```

`Status` serializes internally tagged (`{"kind": "ok"}`, `{"kind": "rate_limited", "until": …}`,
`{"kind": "error", "message": …}`) so the TypeScript side can switch on `kind`.

### Poll loop (`poll.rs`)

One `std::thread` started in `setup`, holding `Arc<Mutex<Snapshot>>` (also in Tauri managed
state) and a `PollState { error_count: u32, last_success: Option<i64>, latched_fingerprint:
Option<u64> }`.

Cycle:

1. `read_credentials()`. None → `Status::NoToken`, delay 30s.
2. If `latched_fingerprint == Some(fingerprint)` → keep `AuthExpired`, delay 30s (no request).
3. Cooldown: if `now - last_success < 120` → delay until that boundary.
4. Fetch. On outcome:
   - Ok → `normalize`, set quotas and `fetched_at`, `error_count = 0`, clear latch,
     `Status::Ok`. Delay = 180s, or `seconds_until(nearest resets_at) + 5` if that is smaller
     and positive (reset-aligned poll).
   - `Unauthorized` → `AuthExpired`, latch fingerprint, delay 30s.
   - `RateLimited { retry_after }` → `error_count += 1`; delay = `retry_after` clamped to
     `[180, 900]` if present, else `min(180 * 2^(error_count - 1), 900)`. Status
     `RateLimited { until: now + delay }`.
   - `Server` / `Network` → `error_count += 1`, `Status::Error { message }`, delay 30s.
5. Set `next_poll_at = now + delay`, emit `usage`, refresh tray title, sleep.

`next_delay(outcome, error_count, retry_after, nearest_reset, now) -> u64` is a pure function
with a table test. Clock-jump guard: if `now < last_success`, treat cooldown as satisfied.

The poll thread sleeps in 60s slices and re-renders the tray title (and re-emits the snapshot)
after each slice, so the countdown ticks without network traffic and without a second thread.

## Tray and popover (`tray.rs`, `main.rs`, `tauri.conf.json`)

### Tray

- `TrayIconBuilder` with a monochrome template icon (`icon_as_template(true)`) and a title.
- `title(snapshot) -> String`:
  - `Ok`: `⏱ {session%} ↻{countdown} · 📅 {weekly%} ↻{countdown}`. Missing quota → that half
    shows `—`.
  - `NoToken`: `⏱ —`
  - `AuthExpired`: `⏱ ! login`
  - `RateLimited`: the `Ok` string with ` (429)` appended
  - `Error`: `⏱ ! err`
  - Per-model quotas never appear in the title.
- Countdown formats: `<1m`, `NNm`, `NhMMm`, `NdNh`.
- `menu_on_left_click(false)`. Right-click menu: `Open usage page`, `Quit`.
- Left click (`TrayIconEvent::Click` with `rect`): position the popover centered under the icon
  (`x = icon_center_x - width / 2`, `y = icon_bottom + 6`, in the icon's monitor coordinates),
  then `show` + `set_focus`. If already visible, hide.

### Popover window

`tauri.conf.json` window `popover`: `visible: false`, `decorations: false`,
`transparent: true`, `alwaysOnTop: true`, `skipTaskbar: true`, `resizable: false`, `width: 320`,
`height: 240` (initial; frontend resizes). `WindowEvent::Focused(false)` → hide.

### macOS presence

`src-tauri/Info.plist` (merged by Tauri) sets `LSUIElement = true`; `setup` calls
`app.set_activation_policy(ActivationPolicy::Accessory)`. `app.macOSPrivateApi` is `true`
because a transparent window on macOS requires it.

### Commands (`main.rs`, all `Result<_, String>`)

- `get_snapshot() -> Snapshot`
- `hide_popover()`
- `resize_popover(height: u32)` — sets window size to `320 × height`
- `quit()`

External links go through `plugin:opener|open_url`, invoked directly from `src/lib/ipc.ts`.

### Capabilities (`capabilities/default.json`, window `popover`)

`core:default` (covers event listening) and `opener:allow-open-url` with
`allow: [{ "url": "https://claude.ai/*" }]`. Hide, resize and quit are app commands, which need
no capability entry.

## React popover

### `lib/ipc.ts`

`getSnapshot()`, `onUsage(cb): () => void`, `hidePopover()`, `resizePopover(height)`,
`openUrl(url)`, `quit()`. When `window.__TAURI_INTERNALS__` is absent (plain `pnpm dev` in a
browser) every function is backed by a fixture snapshot so the UI can be iterated without the
shell.

### `lib/format.ts` (pure, tested)

- `countdown(secs)` → `<1m` / `42m` / `2h13m` / `3d4h`
- `clock(epochSecs, now)` → `16:45` for today, `Sat 21:00` otherwise, local time
- `elapsedPct(quota, now)` → `clamp((period - (resets_at - now)) / period * 100, 0, 100)`
- `barColor(percent, elapsed)` → `over` if `percent >= 100 || percent > elapsed`, `warn` if
  `percent >= 80`, else `ok`
- `relative(secs)` → `just now` / `42s ago` / `3m ago`

### `App.tsx`

State: `snapshot` (from `getSnapshot` then `onUsage`) and `now` (1s interval). Layout:

1. Status banner, only when `status != Ok`: `No Claude Code login found`,
   `Session expired — run claude auth login`, `Rate limited, retrying at 14:52`, or the error text.
2. One card per quota: label, large percent (clamped to 100 for the bar, raw in the number), bar
   with fill, a thin elapsed-time marker, 5 ticks (session) or 7 (weekly), and the line
   `Resets in 2h13m · 16:45`. Colour class from `barColor`. Cards dim to `opacity: .6` when
   `now > next_poll_at + 30`.
3. Links row: `Usage` → `https://claude.ai/settings/usage`, `Billing` →
   `https://claude.ai/settings/billing`, `Quit`.
4. Footer: `Updated 42s ago · next 2m`.

Esc calls `hidePopover()`. A `ResizeObserver` on the root element calls `resizePopover`. Root is
rounded 12px with `backdrop-filter: blur`, colour tokens under `prefers-color-scheme`. No
router, no state library, no components directory until a second screen exists.

## Testing

Rust, inline `#[cfg(test)]`:

- `usage::normalize`: fixture from `docs/research-usage-monitors.md` yields session, weekly and
  one scoped quota; null percent dropped; missing `resets_at` dropped; flat-field fallback when
  `limits` is absent; percent 112 preserved.
- `usage::parse_credentials`: valid blob, empty `claudeAiOauth`, malformed JSON → `None`.
- `poll::next_delay`: table over outcome × error_count × retry_after × nearest reset.
- `tray::title`: one case per `Status`, plus missing-quota `—`.

TypeScript, `test/format.test.ts`: `countdown`, `clock`, `elapsedPct`, `barColor` at the 80/100
and marker boundaries, `relative`.

Network and Keychain paths are exercised manually with `pnpm tauri dev`.

## Error handling and security

- Every failure becomes a `Status`; the last good quotas stay on screen. The UI never shows
  zeros because of an error.
- The token exists only inside `poll.rs`/`usage.rs` call frames. It is never logged, never in
  `Snapshot`, never sent anywhere but `api.anthropic.com`.
- Debug logging via `eprintln!` guarded by `cfg!(debug_assertions)`.

## Scaffolding

- Start from `pnpm create tauri-app` (React + TypeScript template), then trim to the layout above.
- `biome.json` and `tsconfig.json` from the sibling project with the agreed changes
  (`module: ESNext`, `moduleResolution: Bundler`, Biome ignores `src-tauri/`).
- `package.json` scripts: `dev`, `build`, `tauri`, `check`, `check:fix`, `typecheck`,
  `test`, `test:run`, `verify`.
- `tauri.conf.json`: identifier `com.matteo.claude-usage-monitor`, product name
  `Claude Usage Monitor`, bundle targets `app`, `dmg`, `nsis`, `minimumSystemVersion: "12.0"`,
  CSP `default-src 'self'; style-src 'self' 'unsafe-inline'` (bar widths are inline styles).
  Generated placeholder icons until replaced.
- `Cargo.toml`: `tauri` with `tray-icon` and `image-png` features, `tauri-plugin-opener`,
  `reqwest` 0.12 (blocking, rustls-tls-native-roots, json), `serde`, `serde_json`. Release
  profile per CLAUDE.md. Timestamps are parsed by a small hand-written ISO-8601 function; no
  `chrono`.
