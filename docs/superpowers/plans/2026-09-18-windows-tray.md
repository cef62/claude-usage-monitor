# Windows System Tray (v1.4) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run the app on Windows with a two-bar tray icon plus tooltip, the existing menu and popover, and an unsigned x64 NSIS installer built and released by CI.

**Architecture:** A new pure module `icon.rs` renders the 32×32 RGBA tray icon from `Snapshot` + `Settings`. `tray::refresh` (renamed from `refresh_title`) forks once on `cfg(target_os)`: macOS sets the title, everything else sets tooltip + icon. `popover_origin` learns to place the window above a bottom taskbar, and a 250 ms blur guard stops the Windows focus-steal from reopening a popover the tray click just closed. `usage::user_agent` spawns `cmd /C claude --version` on Windows. CI gains a Windows verify+bundle job; the release workflow gains a sequential Windows job.

**Tech Stack:** Tauri 2.11 (`tray-icon`, `tauri::image::Image::new_owned`), Rust stable, GitHub Actions `windows-latest`, `tauri-apps/tauri-action@v0`, NSIS.

**Spec:** `docs/superpowers/specs/2026-09-18-windows-tray-design.md`

## Global Constraints

- macOS behaviour stays byte-for-byte the same: title string, template glyph icon, menu ids, popover position under the menu bar.
- No new crate or npm dependencies. Icon rendering is raw RGBA, no font, no `image` crate.
- Windows target: `x86_64-pc-windows-msvc`, NSIS installer only, unsigned.
- Icon constants (exact): `SIZE = 32`; `OK = [0x34, 0xC7, 0x59, 0xFF]`, `WARN = [0xF5, 0xA6, 0x23, 0xFF]`, `OVER = [0xE5, 0x48, 0x3C, 0xFF]`, `TRACK = [0x80, 0x80, 0x80, 0x90]`, `IDLE = [0xA0, 0xA0, 0xA0, 0xFF]`; bars 26 px wide at x 3..=28, 8 px tall; session y 6..=13, weekly y 18..=25, single bar y 12..=19; 6×6 `OVER` square at x 13..=18, y 13..=18 for `NoToken`/`AuthExpired`; 2 px `OVER` border when `alerts::marker` is true.
- `bar_color(percent, elapsed)` is the `src/lib/format.ts` `barColor` rule: `OVER` when `percent >= 100 || percent > elapsed`, `WARN` when `percent >= 80`, else `OK`.
- Blur guard: `BLUR_GUARD = 400 ms`; a show within that window after a blur-hide is ignored.
- `#[tauri::command]` and event code never `unwrap`/`expect`; `Mutex` locks use `unwrap_or_else(|p| p.into_inner())`.
- `cmd /C claude --version` runs with `CREATE_NO_WINDOW` (`0x0800_0000`) on Windows.
- Commit messages end with `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` as a separate trailer (`git commit -m "<subject>" -m "Co-Authored-By: …"`).
- Run `pnpm verify` before every commit; it must pass on macOS. Windows compilation is proven by CI (Task 5) and, where possible, by `cargo check --target x86_64-pc-windows-msvc` locally.
- Every user-visible change ships with a changeset (Task 6).
- Branch: `feat/windows-tray` (already created, holds the spec). Never commit on `main`.

---

### Task 1: `icon.rs` — pure tray icon renderer

**Files:**
- Create: `src-tauri/src/icon.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod icon;`)

**Interfaces:**
- Consumes: `crate::poll::{Snapshot, Status}`, `crate::settings::Settings`, `crate::usage::Quota`, `crate::alerts::{elapsed_pct, marker}`.
- Produces: `pub const SIZE: u32`, `pub const OK/WARN/OVER/TRACK/IDLE: [u8; 4]`, `pub fn bar_color(percent: f64, elapsed_pct: f64) -> [u8; 4]`, `pub fn render(s: &Snapshot, settings: &Settings, now: i64) -> Vec<u8>` (length `SIZE * SIZE * 4`, row-major RGBA). Task 3 calls `render` and `SIZE`.

- [ ] **Step 1: Register the module and write the failing tests**

Add to `src-tauri/src/lib.rs` after `pub mod alerts;`:

```rust
pub mod icon;
```

Create `src-tauri/src/icon.rs` with only the tests (implementation comes in Step 3):

```rust
//! Windows tray icon: two mini bars (session, weekly) rendered into raw RGBA.
//! Pure so it is unit-testable pixel by pixel; macOS keeps the template glyph and title.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poll::{Snapshot, Status};
    use crate::settings::Settings;
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

    // Session 48.4 % with 2 h 13 m left (elapsed 56 %), weekly 40 % with 3 d left (elapsed 57 %):
    // both behind the clock, so both bars are green.
    fn behind_clock() -> Vec<Quota> {
        vec![
            quota("session", 48.4, 2 * 3600 + 13 * 60, SESSION_SECS),
            quota("weekly", 40.0, 3 * 86400, WEEKLY_SECS),
            quota("weekly:fable", 99.0, 3 * 86400, WEEKLY_SECS),
        ]
    }

    fn snapshot(status: Status, quotas: Vec<Quota>) -> Snapshot {
        Snapshot {
            quotas,
            fetched_at: Some(NOW),
            next_poll_at: NOW + 180,
            status,
        }
    }

    fn px(buf: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn bar_color_follows_the_popover_rule() {
        assert_eq!(bar_color(48.0, 60.0), OK);
        assert_eq!(bar_color(85.0, 60.0), OVER);
        assert_eq!(bar_color(85.0, 90.0), WARN);
        assert_eq!(bar_color(100.0, 100.0), OVER);
    }

    #[test]
    fn render_has_the_right_length() {
        let buf = render(&Snapshot::default(), &Settings::default(), NOW);
        assert_eq!(buf.len(), (SIZE * SIZE * 4) as usize);
    }

    #[test]
    fn two_bars_fill_by_percent_and_ignore_scoped_quotas() {
        let buf = render(&snapshot(Status::Ok, behind_clock()), &Settings::default(), NOW);
        // session: round(26 × 0.484) = 13 px → x 3..=15
        assert_eq!(px(&buf, 3, 8), OK);
        assert_eq!(px(&buf, 15, 8), OK);
        assert_eq!(px(&buf, 16, 8), TRACK);
        assert_eq!(px(&buf, 28, 8), TRACK);
        // weekly: round(26 × 0.40) = 10 px → x 3..=12
        assert_eq!(px(&buf, 12, 20), OK);
        assert_eq!(px(&buf, 13, 20), TRACK);
        // gap between bars and the corners stay transparent
        assert_eq!(px(&buf, 16, 15), [0, 0, 0, 0]);
        assert_eq!(px(&buf, 0, 0), [0, 0, 0, 0]);
        assert_eq!(px(&buf, 2, 8), [0, 0, 0, 0]);
        assert_eq!(px(&buf, 29, 8), [0, 0, 0, 0]);
    }

    #[test]
    fn hidden_weekly_centres_the_session_bar() {
        let mut settings = Settings::default();
        assert!(settings.toggle("weekly"));
        let buf = render(&snapshot(Status::Ok, behind_clock()), &settings, NOW);
        assert_eq!(px(&buf, 3, 8), [0, 0, 0, 0]);
        assert_eq!(px(&buf, 3, 20), [0, 0, 0, 0]);
        assert_eq!(px(&buf, 3, 15), OK);
        assert_eq!(px(&buf, 15, 12), OK);
        assert_eq!(px(&buf, 16, 19), TRACK);
    }

    #[test]
    fn hidden_session_centres_the_weekly_bar() {
        let mut settings = Settings::default();
        assert!(settings.toggle("session"));
        let buf = render(&snapshot(Status::Ok, behind_clock()), &settings, NOW);
        assert_eq!(px(&buf, 12, 15), OK);
        assert_eq!(px(&buf, 13, 15), TRACK);
        assert_eq!(px(&buf, 3, 8), [0, 0, 0, 0]);
    }

    #[test]
    fn missing_quota_draws_the_track_only() {
        let only_session = vec![quota("session", 48.4, 2 * 3600, SESSION_SECS)];
        let buf = render(&snapshot(Status::Ok, only_session), &Settings::default(), NOW);
        assert_eq!(px(&buf, 3, 20), TRACK);
        assert_eq!(px(&buf, 28, 20), TRACK);
    }

    #[test]
    fn ahead_of_clock_is_red_and_over_80_is_amber() {
        let quotas = vec![
            quota("session", 85.0, 4 * 3600, SESSION_SECS), // elapsed 20 % → ahead → red
            quota("weekly", 85.0, 12 * 3600, WEEKLY_SECS),  // elapsed 93 % → behind, ≥ 80 → amber
        ];
        let buf = render(&snapshot(Status::Ok, quotas), &Settings::default(), NOW);
        assert_eq!(px(&buf, 3, 8), OVER);
        assert_eq!(px(&buf, 3, 20), WARN);
    }

    #[test]
    fn alert_marker_draws_a_red_border() {
        let quotas = vec![quota("session", 96.0, 3600, SESSION_SECS)];
        let buf = render(&snapshot(Status::Ok, quotas), &Settings::default(), NOW);
        assert_eq!(px(&buf, 0, 0), OVER);
        assert_eq!(px(&buf, 31, 31), OVER);
        assert_eq!(px(&buf, 1, 16), OVER);
        assert_eq!(px(&buf, 16, 30), OVER);
        assert_eq!(px(&buf, 2, 2), [0, 0, 0, 0]);
    }

    #[test]
    fn no_marker_when_alerts_are_off() {
        let quotas = vec![quota("session", 96.0, 3600, SESSION_SECS)];
        let mut settings = Settings::default();
        assert!(settings.toggle("alert_session"));
        let buf = render(&snapshot(Status::Ok, quotas), &settings, NOW);
        assert_eq!(px(&buf, 0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn no_token_greys_fills_and_draws_the_square() {
        let buf = render(&snapshot(Status::NoToken, behind_clock()), &Settings::default(), NOW);
        assert_eq!(px(&buf, 3, 8), IDLE);
        assert_eq!(px(&buf, 3, 20), IDLE);
        assert_eq!(px(&buf, 16, 16), OVER);
        assert_eq!(px(&buf, 13, 13), OVER);
        assert_eq!(px(&buf, 18, 18), OVER);
        assert_eq!(px(&buf, 12, 16), [0, 0, 0, 0]);
    }

    #[test]
    fn auth_expired_draws_the_square_and_error_only_greys() {
        let expired = render(
            &snapshot(Status::AuthExpired, behind_clock()),
            &Settings::default(),
            NOW,
        );
        assert_eq!(px(&expired, 16, 16), OVER);
        let error = render(
            &snapshot(Status::Error { message: "boom".into() }, behind_clock()),
            &Settings::default(),
            NOW,
        );
        assert_eq!(px(&error, 3, 8), IDLE);
        assert_eq!(px(&error, 16, 16), [0, 0, 0, 0]);
    }

    #[test]
    fn rate_limited_keeps_live_colours() {
        let buf = render(
            &snapshot(Status::RateLimited { until: NOW + 300 }, behind_clock()),
            &Settings::default(),
            NOW,
        );
        assert_eq!(px(&buf, 3, 8), OK);
    }

    #[test]
    fn percent_over_100_clamps_the_fill() {
        let quotas = vec![quota("session", 130.0, 3600, SESSION_SECS)];
        let buf = render(&snapshot(Status::Ok, quotas), &Settings::default(), NOW);
        assert_eq!(px(&buf, 28, 8), OVER);
        assert_eq!(px(&buf, 29, 8), [0, 0, 0, 0]);
    }
}
```

If `Status::RateLimited` or `Status::Error` field names differ from `until` / `message`, check `src-tauri/src/poll.rs` and use the real names — the shapes are `RateLimited { until: i64 }` and `Error { message: String }` as of v0.4.0.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml icon::`
Expected: compile error — `SIZE`, `OK`, `render`, `bar_color` not found.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/src/icon.rs`:

```rust
use crate::alerts;
use crate::poll::{Snapshot, Status};
use crate::settings::Settings;
use crate::usage::Quota;

/// Rendered size; Windows scales it to 16×16 at 100 % DPI.
pub const SIZE: u32 = 32;
pub const OK: [u8; 4] = [0x34, 0xC7, 0x59, 0xFF];
pub const WARN: [u8; 4] = [0xF5, 0xA6, 0x23, 0xFF];
pub const OVER: [u8; 4] = [0xE5, 0x48, 0x3C, 0xFF];
/// Translucent grey so the empty part of a bar reads on both dark and light taskbars.
pub const TRACK: [u8; 4] = [0x80, 0x80, 0x80, 0x90];
/// Fill colour while the numbers are not live (no token, expired, error).
pub const IDLE: [u8; 4] = [0xA0, 0xA0, 0xA0, 0xFF];

const BAR_X: u32 = 3;
const BAR_W: u32 = 26;
const BAR_H: u32 = 8;
const TOP_Y: u32 = 6;
const BOTTOM_Y: u32 = 18;
const SINGLE_Y: u32 = 12;
const SQUARE_XY: u32 = 13;
const SQUARE_SIZE: u32 = 6;
const BORDER: u32 = 2;

/// Same rule as `barColor` in `src/lib/format.ts`, so the icon and the popover never disagree.
pub fn bar_color(percent: f64, elapsed_pct: f64) -> [u8; 4] {
    if percent >= 100.0 || percent > elapsed_pct {
        OVER
    } else if percent >= 80.0 {
        WARN
    } else {
        OK
    }
}

struct Canvas(Vec<u8>);

impl Canvas {
    fn new() -> Self {
        Self(vec![0; (SIZE * SIZE * 4) as usize])
    }

    fn fill(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
        for yy in y..y + h {
            for xx in x..x + w {
                let i = ((yy * SIZE + xx) * 4) as usize;
                self.0[i..i + 4].copy_from_slice(&color);
            }
        }
    }
}

fn bar(canvas: &mut Canvas, y: u32, quota: Option<&Quota>, now: i64, live: bool) {
    canvas.fill(BAR_X, y, BAR_W, BAR_H, TRACK);
    let Some(q) = quota else {
        return;
    };
    let width = (f64::from(BAR_W) * q.percent.clamp(0.0, 100.0) / 100.0).round() as u32;
    let color = if live {
        bar_color(q.percent, alerts::elapsed_pct(q, now))
    } else {
        IDLE
    };
    canvas.fill(BAR_X, y, width, BAR_H, color);
}

/// 32×32 RGBA, row-major. Bars follow the same `session`/`weekly` display settings as the title.
pub fn render(s: &Snapshot, settings: &Settings, now: i64) -> Vec<u8> {
    let mut canvas = Canvas::new();
    let live = matches!(s.status, Status::Ok | Status::RateLimited { .. });
    let find = |key: &str| s.quotas.iter().find(|q| q.key == key);
    // Both halves off is impossible: `Settings::toggle` keeps at least one on.
    match (settings.session, settings.weekly) {
        (true, true) => {
            bar(&mut canvas, TOP_Y, find("session"), now, live);
            bar(&mut canvas, BOTTOM_Y, find("weekly"), now, live);
        }
        (true, false) => bar(&mut canvas, SINGLE_Y, find("session"), now, live),
        (false, _) => bar(&mut canvas, SINGLE_Y, find("weekly"), now, live),
    }
    if matches!(s.status, Status::NoToken | Status::AuthExpired) {
        canvas.fill(SQUARE_XY, SQUARE_XY, SQUARE_SIZE, SQUARE_SIZE, OVER);
    }
    if alerts::marker(&s.quotas, settings) {
        canvas.fill(0, 0, SIZE, BORDER, OVER);
        canvas.fill(0, SIZE - BORDER, SIZE, BORDER, OVER);
        canvas.fill(0, 0, BORDER, SIZE, OVER);
        canvas.fill(SIZE - BORDER, 0, BORDER, SIZE, OVER);
    }
    canvas.0
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml icon::`
Expected: 13 passed.

Then: `pnpm verify` — all green (biome/tsc/vitest untouched; fmt, clippy, cargo test).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/icon.rs src-tauri/src/lib.rs
git commit -m "feat: pure RGBA tray icon renderer for Windows" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: `usage::user_agent` spawns `claude` through `cmd /C` on Windows

**Files:**
- Modify: `src-tauri/src/usage.rs:81-92` (`user_agent`)

**Interfaces:**
- Consumes: nothing new.
- Produces: `fn claude_version_command() -> std::process::Command` (private). `user_agent()` signature unchanged.

- [ ] **Step 1: Confirm the existing test still describes the behaviour**

`usage::tests::user_agent_has_claude_code_prefix_and_numeric_version` already asserts the `claude-code/<digits…>` shape. No new test is possible on macOS for the Windows branch; the Windows CI job (Task 5) runs this same test there.

- [ ] **Step 2: Replace the command construction**

Replace the body of `user_agent` and add the helper directly above it:

```rust
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
```

- [ ] **Step 3: Verify on macOS and, if the target is installable, cross-check the Windows branch**

Run: `pnpm verify`
Expected: green.

Run (best effort):

```bash
rustup target add x86_64-pc-windows-msvc
cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
```

Expected: `Finished`. `cargo check` needs no linker, so it usually works from macOS. If it fails for a reason unrelated to this task's code (missing target, a dependency's build script wanting MSVC tools), note that in the report and rely on Task 5's CI job — do not spend more than one attempt on it.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/usage.rs
git commit -m "fix: resolve the claude .cmd shim through cmd /C on Windows" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: `tray::refresh` — title on macOS, tooltip + icon elsewhere; "Tray" menu label

**Files:**
- Modify: `src-tauri/src/tray.rs` (`use` block, `DISPLAY_LABELS` call site in `setup`, the `TrayIconBuilder` chain, `after_settings_change`, `refresh_title`)
- Modify: `src-tauri/src/main.rs:127` (call site)

**Interfaces:**
- Consumes: `crate::icon::{render, SIZE}` from Task 1.
- Produces: `pub fn refresh(app: &AppHandle, s: &Snapshot)` (replaces `refresh_title`; same semantics on macOS). `const DISPLAY_MENU_LABEL: &str`.

- [ ] **Step 1: Write the failing test**

Append inside `mod tests` in `src-tauri/src/tray.rs`:

```rust
    #[test]
    fn display_menu_label_matches_the_platform() {
        if cfg!(target_os = "macos") {
            assert_eq!(DISPLAY_MENU_LABEL, "Menu bar");
        } else {
            assert_eq!(DISPLAY_MENU_LABEL, "Tray");
        }
    }
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml tray::tests::display_menu_label`
Expected: compile error — `DISPLAY_MENU_LABEL` not found.

- [ ] **Step 2: Add the label constant and use it**

Below `const DISPLAY_LABELS` add:

```rust
/// macOS has a menu bar; every other desktop calls it the tray.
#[cfg(target_os = "macos")]
const DISPLAY_MENU_LABEL: &str = "Menu bar";
#[cfg(not(target_os = "macos"))]
const DISPLAY_MENU_LABEL: &str = "Tray";
```

In `setup`, change

```rust
    let display = check_submenu(app, "Menu bar", &DISPLAY_LABELS, &current, &mut items)?;
```

to

```rust
    let display = check_submenu(app, DISPLAY_MENU_LABEL, &DISPLAY_LABELS, &current, &mut items)?;
```

- [ ] **Step 3: Rename `refresh_title` to `refresh` and fork on platform**

Add `use crate::icon;` to the `use` block (keep the block alphabetical: after `use crate::alerts;`).

Replace the whole `refresh_title` function with:

```rust
/// Pushes the snapshot to the tray. macOS shows the text as the status-item title; Windows has
/// no title, so the same text becomes the tooltip and the numbers are drawn into the icon.
pub fn refresh(app: &AppHandle, s: &Snapshot) {
    let settings = lock_settings(app).clone();
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let text = title(s, poll::now(), &settings);
    #[cfg(target_os = "macos")]
    let _ = tray.set_title(Some(text));
    #[cfg(not(target_os = "macos"))]
    {
        let _ = tray.set_tooltip(Some(text.trim()));
        let rgba = icon::render(s, &settings, poll::now());
        let image = tauri::image::Image::new_owned(rgba, icon::SIZE, icon::SIZE);
        let _ = tray.set_icon(Some(image));
    }
}
```

Update the two callers:

- `src-tauri/src/tray.rs` inside `after_settings_change`: `refresh_title(app, &poll::read(...))` → `refresh(app, &poll::read(...))`.
- `src-tauri/src/main.rs:127`: `tray::refresh_title(&handle, snapshot);` → `tray::refresh(&handle, snapshot);`.

- [ ] **Step 4: Seed the Windows icon at startup**

The builder keeps `.icon(tauri::include_image!("icons/tray.png"))` and `.icon_as_template(true)` (both are what macOS needs; the template flag is a no-op elsewhere). The black template glyph would be invisible on a dark Windows taskbar, so right after `.build(app)?;` in `setup`, before `Ok(())`, add:

```rust
    #[cfg(not(target_os = "macos"))]
    refresh(app, &Snapshot::default());
```

`lock_settings` is not held at that point (`current` is a clone), so this cannot deadlock.

- [ ] **Step 5: Run the tests and verify**

Run: `pnpm verify`
Expected: green; the new test passes with `"Menu bar"` on macOS. `cargo clippy` must not warn about `text` being unused on macOS (it is moved into `set_title`) — if it does, the `cfg` blocks are misplaced.

Run (best effort, same rule as Task 2 Step 3): `cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc`.

Run `pnpm tauri dev` for ~30 s: the macOS menu bar title still renders as before (`◷ 48% ↻2h13m  ·  ▦ 64% ↻3d4h` shape), right-click menu still says "Menu bar". Quit.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/tray.rs src-tauri/src/main.rs
git commit -m "feat: draw the tray icon and tooltip on non-macOS platforms" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: Popover above a bottom taskbar + blur guard

**Files:**
- Modify: `src-tauri/src/tray.rs` (`use` block, `popover_origin`, `toggle_popover`, tests)
- Modify: `src-tauri/src/main.rs` (`on_window_event`, `setup`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `pub fn popover_origin(rect: &Rect, scale: f64, width: f64, height: f64, monitor: LogicalSize<f64>) -> LogicalPosition<f64>`; `pub const BLUR_GUARD: Duration`; `pub fn blur_guard_active(hidden_at: Option<Instant>, now: Instant) -> bool`; `pub struct HiddenAt(pub Mutex<Option<Instant>>)` managed in Tauri state.

- [ ] **Step 1: Write the failing tests**

Replace the existing `popover_is_centered_under_the_icon` test in `src-tauri/src/tray.rs` with these three, and add the blur-guard test:

```rust
    const MONITOR: LogicalSize<f64> = LogicalSize { width: 1440.0, height: 900.0 };

    fn icon_rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            position: Position::Logical(LogicalPosition::new(x, y)),
            size: Size::Logical(LogicalSize::new(w, h)),
        }
    }

    #[test]
    fn popover_is_centered_under_a_menu_bar_icon() {
        let origin = popover_origin(&icon_rect(1000.0, 0.0, 120.0, 22.0), 2.0, 320.0, 240.0, MONITOR);
        assert_eq!(origin.x, 900.0);
        assert_eq!(origin.y, 28.0);
    }

    #[test]
    fn popover_sits_above_a_bottom_taskbar_icon() {
        let origin = popover_origin(&icon_rect(1200.0, 860.0, 24.0, 40.0), 1.0, 320.0, 240.0, MONITOR);
        assert_eq!(origin.x, 1052.0);
        assert_eq!(origin.y, 860.0 - 6.0 - 240.0);
    }

    #[test]
    fn popover_is_clamped_to_the_monitor_edges() {
        let right = popover_origin(&icon_rect(1406.0, 860.0, 24.0, 40.0), 1.0, 320.0, 240.0, MONITOR);
        assert_eq!(right.x, 1440.0 - 320.0);
        let left = popover_origin(&icon_rect(10.0, 0.0, 24.0, 22.0), 1.0, 320.0, 240.0, MONITOR);
        assert_eq!(left.x, 0.0);
    }

    #[test]
    fn blur_guard_only_covers_the_first_250ms() {
        let now = Instant::now();
        assert!(!blur_guard_active(None, now));
        assert!(blur_guard_active(Some(now - Duration::from_millis(100)), now));
        assert!(!blur_guard_active(Some(now - Duration::from_millis(400)), now));
    }
```

Add to the test module's imports: `use std::time::{Duration, Instant};` (and keep the existing `tauri::{LogicalSize, Position, Rect, Size}` line).

Run: `cargo test --manifest-path src-tauri/Cargo.toml tray::tests::popover tray::tests::blur`
Expected: compile errors — wrong arity for `popover_origin`, `blur_guard_active` not found.

- [ ] **Step 2: Implement `popover_origin` and the guard helper**

Add to the `use` block: `use std::time::{Duration, Instant};` and change the `tauri::{…}` import to `use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, Wry};`.

Replace `popover_origin`:

```rust
/// Top-left corner for a `width`×`height` logical-pixel popover next to the tray icon: centred
/// below it when the icon is in the top half of the monitor (menu bar), centred above it
/// otherwise (bottom taskbar), and never past the monitor's left/right edge.
pub fn popover_origin(
    rect: &Rect,
    scale: f64,
    width: f64,
    height: f64,
    monitor: LogicalSize<f64>,
) -> LogicalPosition<f64> {
    let pos = rect.position.to_logical::<f64>(scale);
    let icon = rect.size.to_logical::<f64>(scale);
    let x = (pos.x + icon.width / 2.0 - width / 2.0).clamp(0.0, (monitor.width - width).max(0.0));
    let y = if pos.y + icon.height / 2.0 < monitor.height / 2.0 {
        pos.y + icon.height + POPOVER_GAP
    } else {
        pos.y - POPOVER_GAP - height
    };
    LogicalPosition::new(x, y)
}

/// Clicking the tray icon on Windows first steals focus from the popover, which hides it, and
/// then delivers the click, which would show it again. Ignore shows this soon after a blur-hide.
pub const BLUR_GUARD: Duration = Duration::from_millis(250);

/// When the popover was last hidden because it lost focus.
pub struct HiddenAt(pub Mutex<Option<Instant>>);

pub fn blur_guard_active(hidden_at: Option<Instant>, now: Instant) -> bool {
    hidden_at.is_some_and(|t| now.duration_since(t) < BLUR_GUARD)
}
```

- [ ] **Step 3: Use both in `toggle_popover`**

Replace `toggle_popover`:

```rust
fn toggle_popover(app: &AppHandle, rect: &Rect) {
    let Some(window) = app.get_webview_window("popover") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    let hidden_at = app
        .try_state::<HiddenAt>()
        .and_then(|h| *h.0.lock().unwrap_or_else(|p| p.into_inner()));
    if blur_guard_active(hidden_at, Instant::now()) {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())
        .map(|m| m.size().to_logical::<f64>(m.scale_factor()))
        .unwrap_or_else(|| LogicalSize::new(1920.0, 1080.0));
    let height = window
        .outer_size()
        .map(|s| s.to_logical::<f64>(scale).height)
        .unwrap_or(240.0);
    let _ = window.set_position(popover_origin(rect, scale, POPOVER_WIDTH, height, monitor));
    let _ = window.show();
    let _ = window.set_focus();
}
```

- [ ] **Step 4: Record blur-hides in `main.rs`**

Change the window-event handler to:

```rust
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                let _ = window.hide();
                if let Some(hidden) = window.app_handle().try_state::<tray::HiddenAt>() {
                    *hidden.0.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
                }
            }
        })
```

In `setup`, after `app.manage(Mutex::new(alerts::AlertState::default()));`, add:

```rust
            app.manage(tray::HiddenAt(Mutex::new(None)));
```

Add `use std::time::Instant;` to `main.rs` imports.

- [ ] **Step 5: Verify**

Run: `pnpm verify`
Expected: green, 4 new/updated tray tests pass.

Run `pnpm tauri dev`: click the menu bar item → popover opens centred under it as before; click elsewhere → closes; click the item again → opens (macOS does not blur first, so the guard stays idle). Quit.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/tray.rs src-tauri/src/main.rs
git commit -m "feat: place the popover above a bottom taskbar and guard against blur re-open" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 5: Windows CI job and release job

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/build-release.yml`

**Interfaces:**
- Consumes: the code from Tasks 1–4 compiling on `x86_64-pc-windows-msvc`.
- Produces: CI artifact `windows-installer` on every PR/main run; release asset `Claude.Usage.Monitor_<version>_x64-setup.exe` on every tag.

- [ ] **Step 1: Add the Windows verify job to `ci.yml`**

Append after the `verify-and-build` job (same indentation level under `jobs:`):

```yaml
  verify-windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - uses: pnpm/action-setup@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 24
          cache: pnpm

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-msvc
          components: rustfmt, clippy

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri

      - run: pnpm install --frozen-lockfile

      - run: pnpm verify

      - run: pnpm tauri build --bundles nsis --target x86_64-pc-windows-msvc

      - uses: actions/upload-artifact@v4
        with:
          name: windows-installer
          path: src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe
          retention-days: 7
          if-no-files-found: error
```

- [ ] **Step 2: Add the sequential Windows job to `build-release.yml`**

Append after the `macos` job:

```yaml
  windows:
    needs: macos
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - uses: pnpm/action-setup@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 24
          cache: pnpm

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-msvc

      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri

      - run: pnpm install --frozen-lockfile

      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "Claude Usage Monitor ${{ github.ref_name }}"
          releaseBody: "See CHANGELOG.md for details. macOS: the app is ad-hoc signed, not notarized: on first launch open System Settings → Privacy & Security and click Open Anyway, or run `xattr -cr \"/Applications/Claude Usage Monitor.app\"` once. Windows: the installer is unsigned. When SmartScreen appears, click More info → Run anyway."
          releaseDraft: false
          prerelease: false
          includeUpdaterJson: false
          args: --target x86_64-pc-windows-msvc --bundles nsis
```

Update the `macos` job's `releaseBody` to the identical string (both jobs must agree, since `tauri-action` writes the body when it creates the release and the Windows job only appends assets). `needs: macos` keeps the two runs from racing to create the release.

- [ ] **Step 3: Validate the YAML locally**

Run: `ruby -ryaml -e 'ARGV.each { |f| YAML.load_file(f) }; puts "ok"' .github/workflows/ci.yml .github/workflows/build-release.yml`
Expected: `ok` (macOS ships Ruby with the YAML stdlib). Both files must still parse as two jobs under `jobs:`.

Run `pnpm verify` (unchanged code, still must be green before committing).

- [ ] **Step 4: Commit and push; watch CI**

```bash
git add .github/workflows/ci.yml .github/workflows/build-release.yml
git commit -m "ci: verify, bundle and release the Windows x64 installer" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push -u origin feat/windows-tray
```

Then: `gh run list --branch feat/windows-tray --limit 3` and `gh run watch <id> --exit-status` for the CI run. Expected: both `verify-and-build` and `verify-windows` succeed and the run lists a `windows-installer` artifact (`gh run view <id>` shows it under ARTIFACTS).

If `verify-windows` fails in `pnpm verify` or the build, read the log (`gh run view <id> --log-failed`), fix the Rust/`cfg` issue in the offending file, commit with a `fix:` subject, and push again. Do not weaken the job (no `continue-on-error`).

---

### Task 6: Docs, changeset, PR

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`
- Create: `.changeset/windows-tray.md`

**Interfaces:** none (documentation).

- [ ] **Step 1: README**

Replace the first paragraph:

```markdown
A macOS menu bar / Windows system tray app that shows your Claude plan usage: percentage
consumed in the 5-hour session and weekly windows, and the countdown to each reset. Click it for
a popover with usage bars, an elapsed-time marker, and links to the claude.ai usage and billing
pages.
```

In "How it works", after the paragraph that starts `Menu bar format:`, add:

```markdown
On Windows the tray shows no text, so the icon carries the numbers: the top bar is the session
quota, the bottom bar the weekly quota, green while behind the clock, amber from 80 %, red when
ahead of the clock or full. A red frame means an alert level has been reached; a red square in
the middle means you need to run `claude auth login`. Hover the icon for the same text macOS
shows in the menu bar. The token is read from `%USERPROFILE%\.claude\.credentials.json`
(`%CLAUDE_CONFIG_DIR%` if set).
```

In "Configuration", change the first bullet to:

```markdown
- **Menu bar** (**Tray** on Windows) — Session, Weekly, Glyphs (`◷` / `▦`), Percent, Remaining
  time. At least one quota and one of Percent/Remaining always stay on. On Windows Session/Weekly
  also hide the matching bar in the icon; the other three shape the tooltip.
```

Also change the settings path sentence to mention both locations:

```markdown
Everything lives in the tray right-click menu and persists in
`~/Library/Application Support/com.matteo.claude-usage-monitor/settings.json` (macOS) or
`%APPDATA%\com.matteo.claude-usage-monitor\settings.json` (Windows):
```

In "Releases", replace the sentence about what the tag builds with:

```markdown
The tag builds the Apple Silicon app and the Windows x64 installer and publishes a GitHub
Release with the `.dmg`, `.app.tar.gz` and `-setup.exe`.
```

and add after the macOS "Open Anyway" paragraph:

```markdown
On Windows the installer is unsigned: when SmartScreen appears, click **More info → Run anyway**.
It installs per user (no admin prompt) and needs the WebView2 runtime, which Windows 10/11 ship.
```

Replace "## Not yet" body with:

```markdown
Launch at login, code signing, Windows ARM64, Linux.
```

- [ ] **Step 2: CLAUDE.md**

- Status line → `**Status:** v1.4 (Windows system tray) implemented; released via the Changesets pipeline.` and add `docs/superpowers/specs/2026-09-18-windows-tray-design.md` to the Specs list.
- First paragraph: change `It lives in the macOS menu bar (Windows system tray later)` to `It lives in the macOS menu bar or the Windows system tray`.
- Layout block: add `  src/icon.rs             Windows tray icon renderer (pure RGBA)` after the `src/alerts.rs` line.
- Tauri Rules: change `OS-specific code behind `#[cfg(target_os = "...")]`. macOS first; keep Windows compiling.` to:

```markdown
- OS-specific code behind `#[cfg(target_os = "...")]`. The only platform fork in the tray is
  `tray::refresh` (macOS: title; elsewhere: tooltip + `icon::render`). Spawn `claude` through
  `cmd /C` on Windows (it is a `.cmd` shim) with `CREATE_NO_WINDOW`.
```

- Versioning and Releases, `build-release.yml` line: `… tauri-action builds aarch64-apple-darwin and publishes the GitHub Release, then a second job adds the Windows x64 NSIS installer)`.
- "What NOT to Do": delete the line `Don't scaffold for Windows features before macOS works; keep it compiling, nothing more.`

- [ ] **Step 3: Changeset**

Create `.changeset/windows-tray.md`:

```markdown
---
"claude-usage-monitor": minor
---

Windows support: system tray icon with session/weekly bars and a tooltip, popover above the taskbar, and an unsigned x64 installer attached to every release.
```

- [ ] **Step 4: Verify and commit**

Run: `pnpm verify` (docs only, still required) and `pnpm check` must not complain about the Markdown (Biome ignores `.md`).

```bash
git add README.md CLAUDE.md .changeset/windows-tray.md
git commit -m "docs: Windows tray, installer and settings paths" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push
```

- [ ] **Step 5: Open the PR**

```bash
gh pr create --base main --head feat/windows-tray --title "feat: Windows system tray" --body "$(cat <<'EOF'
## Summary
- Windows tray icon: two bars (session / weekly) rendered in raw RGBA, alert frame, sign-in square; tooltip carries the macOS title text
- Popover opens above a bottom taskbar; 250 ms blur guard so a tray click closes an open popover
- `claude --version` spawned through `cmd /C` on Windows (`.cmd` shim), no console flash
- CI: `verify-windows` job (verify + NSIS bundle, `windows-installer` artifact); release adds a sequential Windows job publishing the unsigned x64 `-setup.exe`
- Docs + changeset (minor)

macOS behaviour unchanged.

## Test plan
- [ ] CI green on both jobs, `windows-installer` artifact present
- [ ] Windows (manual): icon visible on dark and light taskbars; tooltip counts down; left click opens the popover above the icon; click elsewhere closes; click icon again closes; right-click menu shows "Tray ▸"; Send test notification shows a toast; Help → Open log selects the file in Explorer; Quit removes the icon
- [ ] macOS: menu bar title and popover placement unchanged

Spec: `docs/superpowers/specs/2026-09-18-windows-tray-design.md`

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Report the PR URL and the CI run's `windows-installer` artifact URL (`gh run view <id> --json url -q .url`) so the user can install it on Windows.
