# Windows System Tray (v1.4) — Design

Date: 2026-09-18. Status: approved.

## Goal

Run the app on Windows: a system-tray icon that shows both quotas as two mini bars with the full
status in the hover tooltip, the same right-click menu and popover as macOS, an unsigned x64 NSIS
installer built by CI and attached to every release. macOS behaviour stays byte-for-byte the same.

## Decisions

| Topic | Decision |
|---|---|
| Key info on Windows | Tray icon = two stacked bars (session, weekly) drawn in raw RGBA; hover tooltip = the macOS title string. No text in the icon, no font dependency |
| Build targets | `x86_64-pc-windows-msvc`, NSIS installer only, unsigned (SmartScreen "unknown publisher") |
| Verification | CI compiles and bundles; the user installs the CI artifact on a Windows machine and reports |
| Platform split | One new pure module `icon.rs`; one `cfg(target_os)` fork in `tray::refresh`; everything else shared |
| Popover placement | Above the icon when the tray sits in the lower half of the monitor (Windows taskbar), below otherwise (macOS menu bar); clamped to the monitor |
| Blur race | A show requested within 250 ms of a blur-hide is ignored, so a tray click closes an open popover on Windows |

## Out of scope

Launch at login, ARM64 Windows, code signing, Linux, theme-aware icon colours, per-model bars in
the icon, portable `.exe`.

## Components

### `icon.rs` (new, pure)

```rust
pub const SIZE: u32 = 32;                       // rendered 32×32; the OS scales to 16×16 at 100 % DPI
pub const OK: [u8; 4]    = [0x34, 0xC7, 0x59, 0xFF];   // green
pub const WARN: [u8; 4]  = [0xF5, 0xA6, 0x23, 0xFF];   // amber
pub const OVER: [u8; 4]  = [0xE5, 0x48, 0x3C, 0xFF];   // red
pub const TRACK: [u8; 4] = [0x80, 0x80, 0x80, 0x90];   // translucent grey, visible on dark and light taskbars
pub const IDLE: [u8; 4]  = [0xA0, 0xA0, 0xA0, 0xFF];   // grey fill for non-Ok status

pub fn bar_color(percent: f64, elapsed_pct: f64) -> [u8; 4];   // same rule as src/lib/format.ts barColor
pub fn render(s: &Snapshot, settings: &Settings, now: i64) -> Vec<u8>;   // SIZE*SIZE*4 RGBA, row-major
```

Layout (all in pixels of the 32×32 canvas):

- Bars are 26 px wide (x 3..=28), 8 px tall. Two bars: session at y 6..=13, weekly at y 18..=25.
  With one half hidden (`settings.session` / `settings.weekly` off, same keys the title uses) the
  remaining bar is centred at y 12..=19. Both hidden is impossible (`Settings` pair invariant).
- Each bar: `TRACK` full width, then fill from the left `round(26 × clamp(percent, 0, 100) / 100)`
  px in `bar_color(percent, elapsed_pct(quota, now))`. `bar_color`: `OVER` when `percent >= 100`
  or `percent > elapsed_pct`, `WARN` when `percent >= 80`, else `OK`.
- Quota lookup: `key == "session"` and `key == "weekly"`, exactly as `title()`; scoped quotas
  are ignored. A quota missing from the snapshot draws the track only.
- Status other than `Ok` and `RateLimited`: fills use `IDLE`. `NoToken` and `AuthExpired`
  additionally draw a 6×6 `OVER` square centred at (16, 16) on top of everything.
- Alert marker: when `alerts::marker(&s.quotas, settings)` is true, a 2 px `OVER` border is drawn
  around the whole canvas (rows 0–1 and 30–31, columns 0–1 and 30–31).
- Everything outside bars, dot and border is transparent `[0, 0, 0, 0]`.

`elapsed_pct` is the existing `alerts::elapsed_pct`. The module has no Tauri imports beyond
`Snapshot`/`Settings` types and compiles on every platform; only the caller is `cfg`-gated.

### `tray.rs`

- `refresh_title(app, s)` becomes `refresh(app, s)`:

  ```rust
  pub fn refresh(app: &AppHandle, s: &Snapshot) {
      let settings = lock_settings(app).clone();
      let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
      let text = title(s, poll::now(), &settings);
      #[cfg(target_os = "macos")]
      let _ = tray.set_title(Some(text));
      #[cfg(not(target_os = "macos"))]
      {
          let _ = tray.set_tooltip(Some(text.trim()));
          let rgba = icon::render(s, &settings, poll::now());
          let _ = tray.set_icon(Some(tauri::image::Image::new_owned(rgba, icon::SIZE, icon::SIZE)));
      }
  }
  ```

  The 60 s title tick already calls this function, so the tooltip countdown stays current; the
  icon is re-rendered on the same tick (cheap: 4 KB buffer, no allocation beyond it).
- `after_settings_change` already calls `refresh_title`; renamed call. Toggling Session/Weekly
  therefore re-draws the icon.
- Initial icon: `TrayIconBuilder::icon(...)` keeps `icons/tray.png` on macOS (template glyph);
  on Windows `setup` sets the rendered icon of the initial empty snapshot (tracks only) so the
  black template never shows on a dark taskbar. Same `cfg` pattern as above, inside `setup`.
- Menu label: the display submenu is `"Menu bar"` on macOS, `"Tray"` elsewhere
  (`const DISPLAY_MENU_LABEL: &str`, `cfg`-selected). Item ids unchanged.
- `popover_origin(rect, scale, width, height, monitor) -> LogicalPosition<f64>` where `monitor`
  is `LogicalSize<f64>` of the monitor that contains the icon:
  - `x = clamp(rect.center_x − width/2, 0, monitor.width − width)`.
  - `y = rect.y + rect.height + POPOVER_GAP` when `rect.center_y < monitor.height / 2`
    (menu bar at top); else `y = rect.y − POPOVER_GAP − height` (taskbar at bottom).
  - `height` is the popover's current outer height (`window.outer_size()` to logical), so the
    window's bottom edge sits `POPOVER_GAP` above the icon.
- `toggle_popover`: reads `window.current_monitor()` (fallback: `primary_monitor()`, then a
  1920×1080 logical size) and passes its logical size. Before showing, checks the blur guard
  below.

### Blur guard (`main.rs` + `tray.rs`)

- Managed state `pub struct HiddenAt(pub Mutex<Option<Instant>>)`.
- `on_window_event` `Focused(false)`: hide, then store `Instant::now()`.
- `toggle_popover`: if the popover is hidden and `HiddenAt` is within `BLUR_GUARD = 250 ms`, do
  nothing (the click that stole focus already closed it). Pure helper
  `fn blur_guard_active(hidden_at: Option<Instant>, now: Instant) -> bool` for the test.
- Same code on both platforms; on macOS the tray click does not blur the popover first, so the
  guard never triggers.

### `usage.rs`

```rust
#[cfg(target_os = "windows")]
fn claude_version_command() -> std::process::Command {
    let mut c = std::process::Command::new("cmd");
    c.args(["/C", "claude", "--version"]);
    c
}
#[cfg(not(target_os = "windows"))]
fn claude_version_command() -> std::process::Command {
    let mut c = std::process::Command::new("claude");
    c.arg("--version");
    c
}
```

`user_agent` calls `claude_version_command().output()`; the numeric filter and
`FALLBACK_CLI_VERSION` stay. On Windows the process is spawned with
`CREATE_NO_WINDOW` (`std::os::windows::process::CommandExt::creation_flags(0x0800_0000)`) so no
console flashes. Credentials already fall back to `%CLAUDE_CONFIG_DIR%\.credentials.json` /
`%USERPROFILE%\.claude\.credentials.json`; no change.

### `tauri.conf.json`

- Popover window already has `"skipTaskbar": true` (no taskbar button on Windows). `bundle.targets`
  already lists `nsis`; NSIS `installMode` defaults to `currentUser` (no admin prompt). No change.

### CI/release

- `ci.yml`: second job `verify-windows` on `windows-latest`: same steps with target
  `x86_64-pc-windows-msvc`, `pnpm verify`, `pnpm tauri build --bundles nsis --target
  x86_64-pc-windows-msvc`, then `actions/upload-artifact@v4` of
  `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe` named
  `windows-installer` (retention 7 days). This artifact is the manual-test build.
- `build-release.yml`: second job `windows` (`needs: macos`, so the two `tauri-action` runs never
  race to create the release) on `windows-latest` with target `x86_64-pc-windows-msvc` and
  `--bundles nsis`; it uploads to the same tag; `releaseBody` on both adds: "Windows: the installer is
  unsigned. When SmartScreen appears, click More info → Run anyway."
- Windows runners already ship WebView2 and NSIS is downloaded by `tauri-action`; no extra setup
  step.

### Docs and release

- README: "Install" gains a Windows paragraph (SmartScreen, per-user install, tray icon meaning:
  top bar session, bottom bar weekly, red frame = alert, red square = sign in). "Menu bar"
  section notes the Windows name "Tray" and that glyph/percent/remaining apply to the tooltip.
- CLAUDE.md: status line, layout gains `src/icon.rs`, Tauri Rules note the `refresh` fork and
  the `cmd /C` rule for spawning `.cmd` shims on Windows.
- Changeset `minor`.

## Testing

Rust, inline `#[cfg(test)]`:

- `icon::bar_color`: `(48, 60) → OK`, `(85, 60) → OVER` (ahead of clock), `(85, 90) → WARN`,
  `(100, 100) → OVER`.
- `icon::render` with session 48.4 % resetting in 2 h 13 m (elapsed 56 %) and weekly 40 %
  resetting in 3 d (elapsed 57 %), both behind the clock: pixel `(3, 8)` is `OK`, pixel `(15, 8)`
  is `OK` (fill = round(26×0.484) = 13 px → x 3..=15), pixel `(16, 8)` is `TRACK`; weekly row
  `(12, 20)` is `OK` (round(26×0.4) = 10 px → x 3..=12), `(13, 20)` is `TRACK`; corner `(0, 0)` is
  transparent.
- Hidden weekly → row 8 is transparent, row 15 has the session bar.
- `alerts::marker` true (session 96 %, alerts on) → `(0, 0)` and `(31, 31)` are `OVER`.
- `Status::NoToken` → `(16, 16)` is `OVER`, bar fills are `IDLE`.
- Buffer length is exactly `SIZE * SIZE * 4`.
- `tray::popover_origin`: menu-bar rect at y 0 on a 1440×900 monitor → below (existing test
  updated with the new arguments); taskbar rect at y 860, height 40, popover height 240 →
  `y = 860 − 6 − 240`; icon 10 px from the right edge → `x = monitor.width − width`.
- `tray::blur_guard_active`: `None → false`, 100 ms ago → `true`, 400 ms ago → `false`.
- `usage::user_agent` test unchanged (runs the platform command).

CI: both jobs green; the Windows job produces the `windows-installer` artifact.

Manual (user, Windows): install from the PR artifact; tray icon visible on dark and light
taskbars; tooltip shows the title string and counts down; left click opens the popover above
the icon, clicking elsewhere closes it, clicking the icon again closes it; right-click menu
shows "Tray ▸" and every existing item; Send test notification shows a toast; Help → Open log
opens Explorer with the file selected; quitting removes the icon.

## Security notes

No new capabilities, no new network calls, no new dependencies. `cmd /C claude --version` runs
with a fixed argument list (no user input). The token path on Windows is unchanged.
