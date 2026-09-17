# Menu Bar Settings and Title Polish (v1.1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Configurable menu bar title (session/weekly, glyphs, percent, remaining time) from the tray menu, persisted to disk, with monochrome glyphs, wider spacing, and an ad-hoc signed macOS bundle.

**Architecture:** A new `settings.rs` owns a five-boolean `Settings` struct (JSON file in `app_data_dir`, `Mutex<Settings>` in Tauri managed state). `tray::title` takes the settings and renders only the enabled parts. The tray menu gains a "Menu bar" submenu of `CheckMenuItem`s whose handler toggles, re-syncs check marks, saves, and refreshes the title. `tauri.conf.json` gets `signingIdentity: "-"`.

**Tech Stack:** Rust (Tauri 2.11 `menu::{CheckMenuItem, SubmenuBuilder}`, serde), no new crates. No frontend changes.

**Spec:** `docs/superpowers/specs/2026-09-17-menu-bar-settings-design.md`

## Global Constraints

- Branch `feat/menu-bar-settings` (exists, off `main` at `a1a5bc5`). Never commit to `main`.
- Glyphs: session `◷` (U+25F7), weekly `▦` (U+25A6). Separator between halves `"  ·  "` (two spaces, middle dot, two spaces). Every title starts with one leading space.
- Invariants: at least one of `session`/`weekly` and at least one of `percent`/`remaining` enabled at all times. The tray template icon always stays.
- Settings file: `app_data_dir/settings.json`, pretty JSON with trailing newline; missing/invalid → defaults (all true).
- No `unwrap`/`expect` in `settings.rs`, `tray.rs`, or command/setup code. `cargo clippy -- -D warnings` clean. No new dependencies.
- `pnpm verify` green (Biome, tsc, 12 vitest, cargo fmt/clippy/test). Every user-visible change ships with a changeset.
- Commit messages: conventional prefix, normal English, trailer `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`; `git -c commit.gpgsign=false commit` if signing prompts.

---

### Task 1: `settings.rs` — model, invariants, load/save

**Files:**
- Create: `src-tauri/src/settings.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `pub struct Settings { pub session, pub weekly, pub glyph, pub percent, pub remaining: bool }` (`Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default`), `pub const KEYS: [&str; 5]`, `Settings::get(&self, key: &str) -> bool`, `Settings::toggle(&mut self, key: &str) -> bool`, `pub fn load(path: &Path) -> Settings`, `pub fn save(path: &Path, s: &Settings) -> std::io::Result<()>`, `pub fn path(app: &AppHandle) -> Option<PathBuf>`.

- [ ] **Step 1: Declare the module**

`src-tauri/src/lib.rs`:

```rust
//! Library target so integration tests and the binary share the same modules.
pub mod poll;
pub mod settings;
pub mod tray;
pub mod usage;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/settings.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cum-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join("settings.json")
    }

    #[test]
    fn default_is_all_on() {
        let s = Settings::default();
        assert!(KEYS.iter().all(|k| s.get(k)));
    }

    #[test]
    fn toggle_flips_independent_keys() {
        let mut s = Settings::default();
        assert!(s.toggle("glyph"));
        assert!(!s.glyph);
        assert!(s.toggle("glyph"));
        assert!(s.glyph);
        assert!(s.toggle("weekly"));
        assert!(!s.weekly && s.session);
    }

    #[test]
    fn toggle_refuses_to_disable_last_of_each_pair() {
        let mut s = Settings::default();
        assert!(s.toggle("weekly"));
        assert!(!s.toggle("session"), "session is the last enabled quota");
        assert!(s.session);
        assert!(s.toggle("remaining"));
        assert!(!s.toggle("percent"), "percent is the last enabled part");
        assert!(s.percent);
        // Re-enabling the partner makes the other toggleable again.
        assert!(s.toggle("weekly"));
        assert!(s.toggle("session"));
        assert!(!s.session && s.weekly);
    }

    #[test]
    fn toggle_unknown_key_is_noop() {
        let mut s = Settings::default();
        assert!(!s.toggle("colour"));
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn load_missing_or_garbage_returns_default() {
        let p = temp_path("missing");
        assert_eq!(load(&p), Settings::default());
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, "not json").expect("write");
        assert_eq!(load(&p), Settings::default());
    }

    #[test]
    fn save_then_load_round_trips_and_creates_parents() {
        let p = temp_path("roundtrip");
        let mut s = Settings::default();
        s.toggle("glyph");
        s.toggle("weekly");
        save(&p, &s).expect("save");
        let raw = std::fs::read_to_string(&p).expect("read");
        assert!(raw.ends_with('\n'));
        assert!(raw.contains("\"glyph\": false"));
        assert_eq!(load(&p), s);
    }

    #[test]
    fn load_tolerates_missing_and_unknown_fields() {
        let p = temp_path("partial");
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(&p, r#"{"glyph": false, "theme": "dark"}"#).expect("write");
        let s = load(&p);
        assert!(!s.glyph);
        assert!(s.session && s.weekly && s.percent && s.remaining);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml settings 2>&1 | grep -E 'cannot find|unresolved' | head -3`
Expected: `cannot find type `Settings``, `cannot find function `load``.

- [ ] **Step 4: Write the implementation** (above the tests)

```rust
//! Menu bar display settings: which quota halves and which parts of each half to show.
//! Persisted as a small JSON file; every field defaults to true so older or hand-edited files
//! still load.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub session: bool,
    pub weekly: bool,
    pub glyph: bool,
    pub percent: bool,
    pub remaining: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            session: true,
            weekly: true,
            glyph: true,
            percent: true,
            remaining: true,
        }
    }
}

pub const KEYS: [&str; 5] = ["session", "weekly", "glyph", "percent", "remaining"];

impl Settings {
    pub fn get(&self, key: &str) -> bool {
        match key {
            "session" => self.session,
            "weekly" => self.weekly,
            "glyph" => self.glyph,
            "percent" => self.percent,
            "remaining" => self.remaining,
            _ => false,
        }
    }

    /// Flips `key`. Returns false and changes nothing when the flip would disable the last
    /// enabled member of a pair (session/weekly, percent/remaining) or the key is unknown.
    pub fn toggle(&mut self, key: &str) -> bool {
        let partner_on = match key {
            "session" => self.weekly,
            "weekly" => self.session,
            "percent" => self.remaining,
            "remaining" => self.percent,
            "glyph" => true,
            _ => return false,
        };
        if self.get(key) && !partner_on {
            return false;
        }
        match key {
            "session" => self.session = !self.session,
            "weekly" => self.weekly = !self.weekly,
            "glyph" => self.glyph = !self.glyph,
            "percent" => self.percent = !self.percent,
            "remaining" => self.remaining = !self.remaining,
            _ => return false,
        }
        true
    }
}

pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    std::fs::write(path, format!("{json}\n"))
}

pub fn path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("settings.json"))
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml settings`
Expected: `7 passed`.

- [ ] **Step 6: Format and commit**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: `29 passed` (22 existing + 7). Clippy runs in Task 3 once everything is wired (`path` is unused until then).

```bash
git add src-tauri/src/lib.rs src-tauri/src/settings.rs
git commit -m "feat: add persisted menu bar settings model"
```

---

### Task 2: Title rendering honors settings, monochrome glyphs, spacing

**Files:**
- Modify: `src-tauri/src/tray.rs` (`half`, `title`, existing tests)

**Interfaces:**
- Consumes: `crate::settings::Settings`.
- Produces: `pub fn title(s: &Snapshot, now: i64, settings: &Settings) -> String`; `pub const SESSION_GLYPH: &str = "◷"`, `pub const WEEKLY_GLYPH: &str = "▦"`, `pub const SEPARATOR: &str = "  ·  "`.

- [ ] **Step 1: Replace the title tests**

In `src-tauri/src/tray.rs`, inside `mod tests`, add `use crate::settings::Settings;` to the imports and replace the three existing title tests (`title_ok_shows_both_halves_and_ignores_scoped`, `title_missing_quota_shows_dash`, `title_by_status`) with:

```rust
    fn with(keys_off: &[&str]) -> Settings {
        let mut s = Settings::default();
        for k in keys_off {
            assert!(s.toggle(k), "could not disable {k}");
        }
        s
    }

    #[test]
    fn title_all_on_shows_both_halves_and_ignores_scoped() {
        let s = snapshot(Status::Ok, both());
        assert_eq!(
            title(&s, NOW, &Settings::default()),
            " ◷ 48% ↻2h13m  ·  ▦ 64% ↻3d4h"
        );
    }

    #[test]
    fn title_without_glyphs() {
        let s = snapshot(Status::Ok, both());
        assert_eq!(title(&s, NOW, &with(&["glyph"])), " 48% ↻2h13m  ·  64% ↻3d4h");
    }

    #[test]
    fn title_single_half_has_no_separator() {
        let s = snapshot(Status::Ok, both());
        assert_eq!(title(&s, NOW, &with(&["weekly"])), " ◷ 48% ↻2h13m");
        assert_eq!(title(&s, NOW, &with(&["session"])), " ▦ 64% ↻3d4h");
    }

    #[test]
    fn title_percent_or_remaining_only() {
        let s = snapshot(Status::Ok, both());
        assert_eq!(title(&s, NOW, &with(&["remaining"])), " ◷ 48%  ·  ▦ 64%");
        assert_eq!(title(&s, NOW, &with(&["percent"])), " ◷ ↻2h13m  ·  ▦ ↻3d4h");
        assert_eq!(title(&s, NOW, &with(&["percent", "glyph", "weekly"])), " ↻2h13m");
    }

    #[test]
    fn title_missing_quota_shows_dash() {
        let s = snapshot(Status::Ok, vec![quota("session", 48.0, 600, SESSION_SECS)]);
        assert_eq!(title(&s, NOW, &Settings::default()), " ◷ 48% ↻10m  ·  ▦ —");
        assert_eq!(title(&s, NOW, &with(&["glyph"])), " 48% ↻10m  ·  —");
    }

    #[test]
    fn title_by_status() {
        let d = Settings::default();
        let no_glyph = with(&["glyph"]);
        assert_eq!(title(&snapshot(Status::NoToken, vec![]), NOW, &d), " ◷ —");
        assert_eq!(title(&snapshot(Status::NoToken, vec![]), NOW, &no_glyph), " —");
        assert_eq!(title(&snapshot(Status::AuthExpired, both()), NOW, &d), " ◷ ! login");
        assert_eq!(title(&snapshot(Status::AuthExpired, both()), NOW, &no_glyph), " ! login");
        assert_eq!(
            title(&snapshot(Status::Error { message: "x".into() }, both()), NOW, &d),
            " ◷ ! err"
        );
        assert_eq!(
            title(&snapshot(Status::RateLimited { until: NOW + 900 }, both()), NOW, &d),
            " ◷ 48% ↻2h13m  ·  ▦ 64% ↻3d4h (429)"
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml tray 2>&1 | grep -E 'error\[E' | head -3`
Expected: `E0061` "this function takes 2 arguments but 3 arguments were supplied".

- [ ] **Step 3: Rewrite `half` and `title`**

Replace the existing `half` and `title` functions with:

```rust
pub const SESSION_GLYPH: &str = "◷";
pub const WEEKLY_GLYPH: &str = "▦";
pub const SEPARATOR: &str = "  ·  ";

fn half(s: &Snapshot, key: &str, glyph: &str, now: i64, settings: &Settings) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(3);
    if settings.glyph {
        parts.push(glyph.to_string());
    }
    match s.quotas.iter().find(|q| q.key == key) {
        Some(q) => {
            if settings.percent {
                parts.push(format!("{}%", q.percent.round() as i64));
            }
            if settings.remaining {
                parts.push(format!("↻{}", countdown(q.resets_at - now)));
            }
        }
        None => parts.push("—".to_string()),
    }
    parts.join(" ")
}

fn status_text(text: &str, settings: &Settings) -> String {
    if settings.glyph {
        format!(" {SESSION_GLYPH} {text}")
    } else {
        format!(" {text}")
    }
}

pub fn title(s: &Snapshot, now: i64, settings: &Settings) -> String {
    let numbers = || {
        let mut halves = Vec::with_capacity(2);
        if settings.session {
            halves.push(half(s, "session", SESSION_GLYPH, now, settings));
        }
        if settings.weekly {
            halves.push(half(s, "weekly", WEEKLY_GLYPH, now, settings));
        }
        format!(" {}", halves.join(SEPARATOR))
    };
    match s.status {
        Status::Ok => numbers(),
        Status::RateLimited { .. } => format!("{} (429)", numbers()),
        Status::NoToken => status_text("—", settings),
        Status::AuthExpired => status_text("! login", settings),
        Status::Error { .. } => status_text("! err", settings),
    }
}
```

Add `use crate::settings::Settings;` to the imports at the top of `tray.rs`. Temporarily make `refresh_title` compile by passing `&Settings::default()` — Task 3 replaces that with managed state.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: `32 passed` (22 − 3 removed + 6 new + 7 settings).

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
git add src-tauri/src/tray.rs
git commit -m "feat: render menu bar title from settings with monochrome glyphs"
```

---

### Task 3: Check-item submenu, managed state, wiring, signing

**Files:**
- Modify: `src-tauri/src/tray.rs` (`setup`, `refresh_title`, new `MenuItems`), `src-tauri/src/main.rs`, `src-tauri/tauri.conf.json`

**Interfaces:**
- Consumes: `settings::{Settings, KEYS, load, save, path}`, `tray::title`.
- Produces: `pub struct MenuItems(pub HashMap<String, CheckMenuItem<Wry>>)` managed state; `Mutex<Settings>` managed state; menu ids `set:<key>`.

- [ ] **Step 1: Rewrite `setup` and `refresh_title` in `tray.rs`**

Replace the imports block with:

```rust
use crate::poll::{self, Snapshot, Status};
use crate::settings::{self, Settings, KEYS};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, LogicalPosition, Manager, Rect, Wry};
```

Add after the constants:

```rust
/// Check items of the "Menu bar" submenu, kept so the handler can re-sync check marks.
pub struct MenuItems(pub HashMap<String, CheckMenuItem<Wry>>);

const LABELS: [(&str, &str); 5] = [
    ("session", "Session"),
    ("weekly", "Weekly"),
    ("glyph", "Glyphs"),
    ("percent", "Percent"),
    ("remaining", "Remaining time"),
];
```

Replace `setup`:

```rust
pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let current = *lock_settings(app);
    let open = MenuItemBuilder::with_id("open", "Open usage page").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let mut items = HashMap::new();
    let mut submenu = SubmenuBuilder::new(app, "Menu bar");
    for (key, label) in LABELS {
        let item = CheckMenuItem::with_id(app, format!("set:{key}"), label, true, current.get(key), None::<&str>)?;
        submenu = submenu.item(&item);
        items.insert(key.to_string(), item);
    }
    let submenu = submenu.build()?;
    app.manage(MenuItems(items));

    let menu = MenuBuilder::new(app)
        .item(&open)
        .item(&submenu)
        .separator()
        .item(&quit)
        .build()?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .title(title(&Snapshot::default(), poll::now(), &current))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "open" => {
                    let _ = tauri_plugin_opener::open_url(USAGE_URL, None::<&str>);
                }
                "quit" => app.exit(0),
                _ => {
                    if let Some(key) = id.strip_prefix("set:") {
                        on_setting_toggled(app, key);
                    }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_popover(tray.app_handle(), &rect);
            }
        })
        .build(app)?;
    Ok(())
}

fn lock_settings(app: &AppHandle) -> std::sync::MutexGuard<'_, Settings> {
    app.state::<Mutex<Settings>>()
        .inner()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn on_setting_toggled(app: &AppHandle, key: &str) {
    let updated = {
        let mut s = lock_settings(app);
        s.toggle(key);
        *s
    };
    // Re-sync every check mark so a refused toggle snaps back.
    if let Some(items) = app.try_state::<MenuItems>() {
        for k in KEYS {
            if let Some(item) = items.0.get(k) {
                let _ = item.set_checked(updated.get(k));
            }
        }
    }
    if let Some(path) = settings::path(app) {
        // The in-memory value already applies; a failed write only loses persistence.
        let _ = settings::save(&path, &updated);
    }
    refresh_title(app, &poll::read(&app.state::<poll::Shared>()));
}

pub fn refresh_title(app: &AppHandle, s: &Snapshot) {
    let settings = *lock_settings(app);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(Some(title(s, poll::now(), &settings)));
    }
}
```

Note: `app.state::<Mutex<Settings>>()` panics if the state was never managed; `main.rs` manages it before `tray::setup` runs (Step 2), which is the invariant this relies on. `MutexGuard` borrows the `State`; the `.inner()` call returns `&Mutex<Settings>` tied to the app handle's lifetime, which is what the return type needs. If the borrow checker rejects `lock_settings` as written, inline the three lines at each of the three call sites instead.

- [ ] **Step 2: Wire `main.rs`**

In `main.rs`: add `use claude_usage_monitor::settings;` and `use std::sync::Mutex` (already imported as part of `std::sync::{Arc, Mutex}`). In `main()`, replace the `.setup(move |app| { ... })` block with:

```rust
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let initial = settings::path(app.handle())
                .map(|p| settings::load(&p))
                .unwrap_or_default();
            app.manage(Mutex::new(initial));
            tray::setup(app.handle())?;
            let handle = app.handle().clone();
            poll::run(shared, move |snapshot| {
                tray::refresh_title(&handle, snapshot);
                let _ = handle.emit("usage", snapshot);
            });
            Ok(())
        })
```

- [ ] **Step 3: Ad-hoc signing**

In `src-tauri/tauri.conf.json`, under `bundle.macOS`, add `"signingIdentity": "-"` so it reads:

```json
    "macOS": {
      "minimumSystemVersion": "13.0",
      "signingIdentity": "-"
    },
```

- [ ] **Step 4: Build, lint, test**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: clippy clean, `32 passed`. If a Tauri menu API name differs in 2.11 (`SubmenuBuilder::new(app, text)`, `.item(&x)`, `.build()`, `CheckMenuItem::with_id(app, id, text, enabled, checked, accelerator)`, `set_checked`), check `~/.cargo/registry/src/*/tauri-2.11.*/src/menu/` and use the documented name; note it in the report.

- [ ] **Step 5: Manual check**

Run: `pnpm tauri dev` (background, ~3 minutes). Expected: title ` ◷ NN% ↻… · ▦ NN% ↻…` with the wider gap. Right-click → "Menu bar" submenu shows five checked items. Toggle "Glyphs": title loses `◷`/`▦` immediately. Toggle "Weekly": only the session half remains; then try to uncheck "Session": it stays checked. Quit, relaunch: the choices persist (`~/Library/Application Support/com.matteo.claude-usage-monitor/settings.json` exists). If you cannot click menus from your environment, verify persistence by writing that file with `{"glyph": false}` before launch and checking the title has no glyphs; report the click checks as human-pending.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/tray.rs src-tauri/src/main.rs src-tauri/tauri.conf.json
git commit -m "feat: menu bar settings submenu with persistence and ad-hoc signed bundle"
```

---

### Task 4: Docs, changeset, PR

**Files:**
- Modify: `README.md`, `CLAUDE.md`
- Create: `.changeset/menu-bar-settings.md`

- [ ] **Step 1: README**

Under "## How it works", after the "Menu bar format" paragraph, add:

```markdown
Right-click the menu bar item → **Menu bar** to choose what it shows: Session, Weekly, Glyphs
(`◷` / `▦`), Percent, Remaining time. At least one quota and one of Percent/Remaining always
stay on. Choices persist in `~/Library/Application Support/com.matteo.claude-usage-monitor/settings.json`.
```

Change the "Menu bar format" line to: ``Menu bar format: `◷ 48% ↻2h13m  ·  ▦ 64% ↻3d4h` (session · weekly).`` keeping the ` (429)` sentence.

Replace the unsigned-app sentence in "## Releases" (and the one in "## Development") with:

```markdown
The app is ad-hoc signed, not notarized. On first launch macOS says it cannot verify the
developer: open **System Settings → Privacy & Security** and click **Open Anyway**, or run
`xattr -cr "/Applications/Claude Usage Monitor.app"` once.
```

- [ ] **Step 2: CLAUDE.md**

In the Layout block add after `src/tray.rs`: `  src/settings.rs          menu bar display settings, JSON in app_data_dir`. In "Tauri Rules" change "The app is unsigned until told otherwise." to "The app is ad-hoc signed (`signingIdentity: "-"`), not notarized, until an Apple Developer account exists." Update the `**Status:**` paragraph to mention v1.1 (menu bar settings) on `feat/menu-bar-settings`.

- [ ] **Step 3: Changeset**

`.changeset/menu-bar-settings.md`:

```markdown
---
"claude-usage-monitor": minor
---

Configurable menu bar: choose Session/Weekly, Glyphs, Percent and Remaining time from the tray menu (persisted). Monochrome glyphs and wider spacing. The macOS bundle is now ad-hoc signed so downloaded builds open via "Open Anyway" instead of reporting "damaged".
```

- [ ] **Step 4: Verify, commit, push, PR**

Run: `pnpm verify 2>&1 | tail -3 && pnpm changeset status`
Expected: green, 12 vitest, 32 cargo; changeset lists a `minor` bump.

```bash
git add README.md CLAUDE.md .changeset/menu-bar-settings.md
git commit -m "docs: describe menu bar settings and ad-hoc signing"
git push -u origin feat/menu-bar-settings
gh pr create --base main --head feat/menu-bar-settings --title "feat: menu bar settings, monochrome glyphs, ad-hoc signing" --body-file - <<'EOF'
## Summary

- Tray right-click → **Menu bar** submenu: Session, Weekly, Glyphs, Percent, Remaining time (check items, persisted to `app_data_dir/settings.json`; at least one quota and one of Percent/Remaining always on).
- Monochrome glyphs `◷` / `▦` replace the colour emoji; wider spacing (` ◷ 13% ↻3h44m  ·  ▦ 9% ↻1d22h`).
- `signingIdentity: "-"`: the bundle is ad-hoc signed so downloaded builds open via "Open Anyway" instead of "damaged".

Spec: `docs/superpowers/specs/2026-09-17-menu-bar-settings-design.md`

## Test plan

- [x] `pnpm verify` (12 vitest, 32 cargo)
- [x] Settings toggle/persist verified in `tauri dev`
- [ ] Next release asset: `codesign -dv` shows a full ad-hoc signature; opens on another Mac after Open Anyway

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
gh pr checks --watch --interval 30
```

Expected: CI `verify-and-build` passes.
