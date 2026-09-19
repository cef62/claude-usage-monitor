# Auto-update (v1.7) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Installed builds check GitHub Releases daily, announce a newer version once, and install it from a tray menu item.

**Architecture:** `tauri-plugin-updater` (Rust side only) reads `latest.json` from the latest GitHub Release and verifies minisign signatures against the public key in `tauri.conf.json`. A new `update.rs` owns the state (`UpdateState`), the background checker thread, the manual check and the install; `tray.rs` adds two Help items; `main.rs` wires the plugin and state. CI signs updater artifacts with the repo secrets and publishes `latest.json`.

**Tech Stack:** Tauri 2.11, `tauri-plugin-updater` 2.11 (already in `Cargo.toml`/`Cargo.lock` on this branch), `tauri-apps/tauri-action@v0` `includeUpdaterJson`, minisign keys in `~/.tauri/` and repo secrets (already set).

**Spec:** `docs/superpowers/specs/2026-09-19-auto-update-design.md`

## Global Constraints

- Rust side only: no npm package, no capability entry, no frontend change.
- Debug builds never check for updates (`cfg!(debug_assertions)` short-circuit in `spawn_checker`; the manual item still works in debug so it can be exercised, but hits the real feed).
- Cadence: `FIRST_CHECK_DELAY = 30 s`, `CHECK_INTERVAL = 24 h`; const assert `FIRST_CHECK_DELAY < CHECK_INTERVAL`.
- Notification copy (exact): available → title `Claude Usage Monitor {v} available`, body `Right-click the tray icon → Help → Install update`; up to date (manual only) → title `Up to date ({current})`, no body; check failed (manual only) → title `Update check failed`, body `{err}`; busy (manual only) → title `Update in progress…`; install failed → title `Update failed`, body `{err}`.
- Log lines (exact prefixes): `update available {v}`, `update none`, `update check failed {err}`, `update installing {v}`, `update download {pct}%` (at 25/50/75/100), `update installed {v}, restarting`, `update install failed {err}`.
- Menu ids: `check-updates`, `install-update`. Install item text: `Install update…` disabled when nothing is stored; `Install update {v}…` enabled when an update is stored.
- The `UpdateState` mutex is never held across a network call or `app.restart()`.
- Endpoint: `https://github.com/cef62/claude-usage-monitor/releases/latest/download/latest.json`. Windows `installMode: "passive"`. `bundle.createUpdaterArtifacts: true`.
- Secrets are already set on the repo: `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Never print, copy or commit `~/.tauri/claude-usage-monitor.key` or `.password`. Only `~/.tauri/claude-usage-monitor.key.pub` is read.
- No `unwrap`/`expect` outside tests; locks via `unwrap_or_else(|p| p.into_inner())`.
- `pnpm verify` green before every commit; commit trailer `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` as a second `-m`.
- Branch: `feat/auto-update` (exists, holds the spec, `Cargo.toml` + `Cargo.lock` already contain `tauri-plugin-updater = "2"`, uncommitted).

---

### Task 1: `update.rs` skeleton, plugin config, plugin registration

**Files:**
- Create: `src-tauri/src/update.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod update;`)
- Modify: `src-tauri/src/main.rs` (plugin + managed state)
- Modify: `src-tauri/tauri.conf.json` (`bundle.createUpdaterArtifacts`, `plugins.updater`)
- Commit (already modified on disk): `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`

**Interfaces:**
- Produces: `pub const FIRST_CHECK_DELAY: Duration`, `pub const CHECK_INTERVAL: Duration`, `pub struct UpdateState { pub available: Option<tauri_plugin_updater::Update>, pub notified: Option<String>, pub busy: bool }` (Default), `pub enum Trigger { Auto, Manual }`, `pub fn should_notify(version: &str, notified: Option<&str>) -> bool`. Managed state `Mutex<update::UpdateState>` in `main.rs`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/update.rs` with only this content:

```rust
//! In-app updates: daily check against the GitHub Release feed, one notification per version,
//! install on request from the tray menu. Delivery goes through tauri-plugin-updater.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifies_each_version_once() {
        assert!(should_notify("0.8.0", None));
        assert!(!should_notify("0.8.0", Some("0.8.0")));
        assert!(should_notify("0.8.1", Some("0.8.0")));
    }

    #[test]
    fn first_check_comes_before_the_interval() {
        assert!(FIRST_CHECK_DELAY < CHECK_INTERVAL);
    }
}
```

Add `pub mod update;` to `src-tauri/src/lib.rs` after `pub mod tray;` (keep the list alphabetical: `alerts, icon, log, poll, settings, tray, update, usage`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml update::`
Expected: compile error — `should_notify`, `FIRST_CHECK_DELAY`, `CHECK_INTERVAL` not found.

- [ ] **Step 3: Write the skeleton**

Insert above the `#[cfg(test)]` block in `src-tauri/src/update.rs`:

```rust
use std::time::Duration;
use tauri_plugin_updater::Update;

/// Let the first usage poll finish before touching GitHub.
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);
const _: () = assert!(FIRST_CHECK_DELAY.as_secs() < CHECK_INTERVAL.as_secs());

/// What the last check found. The plugin's `Update` handle is kept so "Install" needs no
/// second round-trip to the feed.
#[derive(Default)]
pub struct UpdateState {
    pub available: Option<Update>,
    /// Version already announced by a notification; announce each version once.
    pub notified: Option<String>,
    /// A check or an install is running; a second request is dropped.
    pub busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trigger {
    Auto,
    Manual,
}

pub fn should_notify(version: &str, notified: Option<&str>) -> bool {
    notified != Some(version)
}
```

- [ ] **Step 4: Register the plugin and the state; add the config**

`src-tauri/src/main.rs`: after the autostart plugin line add

```rust
        .plugin(tauri_plugin_updater::Builder::new().build())
```

and inside `setup`, right after `app.manage(tray::HiddenAt(Mutex::new(None)));`, add

```rust
            app.manage(Mutex::new(update::UpdateState::default()));
```

with `update` added to the `use claude_usage_monitor::{alerts, log, settings};` line (alphabetical: `{alerts, log, settings, update}`).

`src-tauri/tauri.conf.json`: inside `"bundle"` add `"createUpdaterArtifacts": true,` as the first key, and add a top-level `"plugins"` object after `"bundle"`:

```json
  "plugins": {
    "updater": {
      "pubkey": "<PUBKEY>",
      "endpoints": [
        "https://github.com/cef62/claude-usage-monitor/releases/latest/download/latest.json"
      ],
      "windows": { "installMode": "passive" }
    }
  }
```

where `<PUBKEY>` is the exact single-line content of `~/.tauri/claude-usage-monitor.key.pub` (`cat ~/.tauri/claude-usage-monitor.key.pub` — it is a base64 string starting with `dW50cnVzdGVkIGNvbW1lbnQ6`; strip the trailing newline). Never read the `.key` or `.password` files.

Also check `scripts/sync-version.mjs` still matches: it replaces `"version": "x.y.z"` by string; the new keys do not contain `"version"`, so nothing changes. Run `pnpm test:run` to confirm `test/sync-version.test.ts` still passes.

- [ ] **Step 5: Run the tests and verify**

Run: `cargo test --manifest-path src-tauri/Cargo.toml update::`
Expected: 2 passed.

Run: `pnpm verify` — green. The build must compile with the plugin config present (Tauri validates `plugins.updater` at build time; a malformed pubkey fails here, not at runtime).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/update.rs src-tauri/src/lib.rs src-tauri/src/main.rs src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: updater plugin, public key and update state skeleton" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 2: Help menu items and actions

**Files:**
- Modify: `src-tauri/src/tray.rs` (`MenuAction`, `parse_menu_id`, `setup` Help submenu, menu handler, tests)

**Interfaces:**
- Consumes: nothing from Task 1 yet (handlers call `update::check` / `update::install`, which Task 3 adds — this task compiles by calling them only after Task 3; see Step 3 for the order).
- Produces: `pub struct UpdateItem(pub MenuItem<Wry>)` managed state; `MenuAction::CheckUpdates`, `MenuAction::InstallUpdate`; ids `check-updates`, `install-update`.

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/tray.rs` test `menu_ids_round_trip`, after the `autostart` assertion add:

```rust
        assert!(matches!(
            parse_menu_id("check-updates"),
            Some(MenuAction::CheckUpdates)
        ));
        assert!(matches!(
            parse_menu_id("install-update"),
            Some(MenuAction::InstallUpdate)
        ));
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml menu_ids`
Expected: compile error — no variants `CheckUpdates` / `InstallUpdate`.

- [ ] **Step 2: Add the variants, ids, items and handlers**

`MenuAction` gains `CheckUpdates,` and `InstallUpdate,` after `Autostart,`.

`parse_menu_id` match gains:

```rust
        "check-updates" => return Some(MenuAction::CheckUpdates),
        "install-update" => return Some(MenuAction::InstallUpdate),
```

Below `pub struct LastTrayRect` add:

```rust
/// The "Install update…" item: text and enabled state follow `update::UpdateState`.
pub struct UpdateItem(pub MenuItem<Wry>);
```

and add `MenuItem` to the `tauri::menu::{…}` import.

In `setup`, replace the Help submenu construction with:

```rust
    let open_log_item = MenuItemBuilder::with_id("open-log", "Open log").build(app)?;
    let check_updates = MenuItemBuilder::with_id("check-updates", "Check for updates…").build(app)?;
    let install_update = MenuItemBuilder::with_id("install-update", "Install update…")
        .enabled(false)
        .build(app)?;
    app.manage(UpdateItem(install_update.clone()));
    let help = SubmenuBuilder::new(app, "Help")
        .item(&open_log_item)
        .separator()
        .item(&check_updates)
        .item(&install_update)
        .build()?;
```

In the `on_menu_event` match add, before `None => {}`:

```rust
            Some(MenuAction::CheckUpdates) => {
                let app = app.clone();
                std::thread::spawn(move || update::check(&app, update::Trigger::Manual));
            }
            Some(MenuAction::InstallUpdate) => {
                let app = app.clone();
                std::thread::spawn(move || update::install(&app));
            }
```

and `use crate::update;` in the `use` block (alphabetical, after `use crate::settings::{…};`).

- [ ] **Step 3: Temporarily stub the two functions so this task compiles on its own**

Task 3 replaces these. Append to `src-tauri/src/update.rs` (above the tests):

```rust
pub fn check(_app: &tauri::AppHandle, _trigger: Trigger) {}
pub fn install(_app: &tauri::AppHandle) {}
```

- [ ] **Step 4: Run the tests and verify**

Run: `cargo test --manifest-path src-tauri/Cargo.toml menu_ids`
Expected: PASS.

Run: `pnpm verify` — green (clippy must not complain about the unused stub parameters: the underscore prefixes cover it).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tray.rs src-tauri/src/update.rs
git commit -m "feat: Help menu items for checking and installing updates" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 3: Check, install, background checker

**Files:**
- Modify: `src-tauri/src/update.rs` (replace the stubs)
- Modify: `src-tauri/src/main.rs` (`spawn_checker`)

**Interfaces:**
- Consumes: `tray::UpdateItem` (Task 2), `log::write(app, &str)`, `tauri_plugin_notification::NotificationExt`, `tauri_plugin_updater::UpdaterExt`.
- Produces: `pub fn spawn_checker(app: AppHandle)`, `pub fn check(app: &AppHandle, trigger: Trigger)`, `pub fn install(app: &AppHandle)`, `pub fn set_install_item(app: &AppHandle, version: Option<&str>)`.

- [ ] **Step 1: Write the failing test for the item text rule**

The only new pure piece is the item label. Add to the tests in `update.rs`:

```rust
    #[test]
    fn install_item_text_names_the_version() {
        assert_eq!(install_item_text(None), "Install update…");
        assert_eq!(install_item_text(Some("0.8.1")), "Install update 0.8.1…");
    }
```

Run: `cargo test --manifest-path src-tauri/Cargo.toml update::`
Expected: compile error — `install_item_text` not found.

- [ ] **Step 2: Replace the stubs with the implementation**

Delete the two stub functions from Task 2 and add (above the tests):

```rust
use crate::log;
use crate::tray::UpdateItem;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;
```

(merge with the Task 1 imports: one `use tauri_plugin_updater::{Update, UpdaterExt};` line)

```rust

fn state(app: &AppHandle) -> MutexGuard<'_, UpdateState> {
    app.state::<Mutex<UpdateState>>()
        .inner()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let mut n = app.notification().builder().title(title);
    if !body.is_empty() {
        n = n.body(body);
    }
    let _ = n.show();
}

pub fn install_item_text(version: Option<&str>) -> String {
    match version {
        Some(v) => format!("Install update {v}…"),
        None => "Install update…".to_string(),
    }
}

/// Mirrors the stored update onto the Help menu item.
pub fn set_install_item(app: &AppHandle, version: Option<&str>) {
    if let Some(item) = app.try_state::<UpdateItem>() {
        let _ = item.0.set_text(install_item_text(version));
        let _ = item.0.set_enabled(version.is_some());
    }
}

/// Claims the busy flag; `false` means another check/install is already running.
fn begin(app: &AppHandle) -> bool {
    let mut st = state(app);
    if st.busy {
        return false;
    }
    st.busy = true;
    true
}

/// Release builds only: first check after `FIRST_CHECK_DELAY`, then every `CHECK_INTERVAL`.
pub fn spawn_checker(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        loop {
            check(&app, Trigger::Auto);
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

pub fn check(app: &AppHandle, trigger: Trigger) {
    let manual = trigger == Trigger::Manual;
    if !begin(app) {
        if manual {
            notify(app, "Already checking…", "");
        }
        return;
    }
    // The lock is released here; the network call runs without it.
    let result = tauri::async_runtime::block_on(async { app.updater()?.check().await });
    let mut st = state(app);
    st.busy = false;
    match result {
        Ok(Some(update)) => {
            let version = update.version.clone();
            st.available = Some(update);
            let announce = manual || should_notify(&version, st.notified.as_deref());
            if announce {
                st.notified = Some(version.clone());
            }
            drop(st);
            set_install_item(app, Some(&version));
            if announce {
                notify(
                    app,
                    &format!("Claude Usage Monitor {version} available"),
                    "Right-click the tray icon → Help → Install update",
                );
            }
            log::write(app, &format!("update available {version}"));
        }
        Ok(None) => {
            st.available = None;
            drop(st);
            set_install_item(app, None);
            if manual {
                let current = app.package_info().version.to_string();
                notify(app, &format!("Up to date ({current})"), "");
            }
            log::write(app, "update none");
        }
        Err(e) => {
            drop(st);
            if manual {
                notify(app, "Update check failed", &e.to_string());
            }
            log::write(app, &format!("update check failed {e}"));
        }
    }
}

/// Downloads and installs the stored update, then relaunches. On Windows the NSIS installer
/// ends the process itself; `restart` is still reached only on macOS.
pub fn install(app: &AppHandle) {
    let update = {
        let mut st = state(app);
        if st.busy {
            return;
        }
        let Some(update) = st.available.clone() else {
            return;
        };
        st.busy = true;
        update
    };
    let version = update.version.clone();
    log::write(app, &format!("update installing {version}"));
    let mut received: u64 = 0;
    let mut last_quarter: u64 = 0;
    let result = tauri::async_runtime::block_on(update.download_and_install(
        |chunk, total| {
            received += chunk as u64;
            if let Some(total) = total.filter(|t| *t > 0) {
                let quarter = received * 4 / total;
                if quarter > last_quarter {
                    last_quarter = quarter;
                    log::write(app, &format!("update download {}%", quarter * 25));
                }
            }
        },
        || {},
    ));
    state(app).busy = false;
    match result {
        Ok(()) => {
            log::write(app, &format!("update installed {version}, restarting"));
            app.restart();
        }
        Err(e) => {
            state(app).available = None;
            set_install_item(app, None);
            notify(app, "Update failed", &e.to_string());
            log::write(app, &format!("update install failed {e}"));
        }
    }
}
```

`app.restart()` returns `!`; if clippy flags the `match` arm types, write `Ok(()) => { …; app.restart() }` without a trailing semicolon after `restart()`.

- [ ] **Step 3: Start the checker**

`src-tauri/src/main.rs` `setup`, after `tray::setup(app.handle())?;`, add:

```rust
            update::spawn_checker(app.handle().clone());
```

- [ ] **Step 4: Run tests, verify, smoke**

Run: `cargo test --manifest-path src-tauri/Cargo.toml update::` — 3 passed.
Run: `pnpm verify` — green.
Run: `pnpm tauri dev` for ~40 s, then Ctrl-C (or kill the `claude-usage-monitor` and `pnpm` processes): the log file (`~/Library/Application Support/com.matteo.claude-usage-monitor/claude-usage-monitor.log`) must contain NO `update` line (debug build skips the checker). Optionally right-click → Help → Check for updates… and confirm a notification `Update check failed` (repo private → 404) or `Up to date` and a matching log line; both prove the plumbing.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/update.rs src-tauri/src/main.rs
git commit -m "feat: daily update check, notification and one-click install" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

### Task 4: CI signing, docs, changeset, PR

**Files:**
- Modify: `.github/workflows/build-release.yml` (matrix `tauri-action` step)
- Modify: `README.md`, `CONTRIBUTING.md`, `CLAUDE.md`
- Create: `.changeset/auto-update.md`

**Interfaces:** none.

- [ ] **Step 1: CI**

In `.github/workflows/build-release.yml`, the `tauri-apps/tauri-action@v0` step becomes:

```yaml
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          releaseId: ${{ needs.create-release.outputs.id }}
          includeUpdaterJson: true
          args: ${{ matrix.args }}
```

`ci.yml` stays as is. Validate: `ruby -ryaml -e 'ARGV.each { |f| YAML.load_file(f) }; puts "ok"' .github/workflows/build-release.yml`.

- [ ] **Step 2: README**

Add a section after "### Alerts" (before "## Configure"):

```markdown
### Updates

The app checks the [Releases page](https://github.com/cef62/claude-usage-monitor/releases/latest)
30 seconds after launch and then once a day. When a newer version exists you get one
notification per version and **Help → Install update x.y.z…** appears in the tray menu: click it
to download, install and relaunch (Windows shows the installer's progress bar). **Help → Check
for updates…** checks on demand and reports the result as a notification. Update packages are
signature-checked against a key built into the app, so only releases from this repository
install. Builds older than 0.8.0 have no updater: install 0.8.0 by hand once.
```

Update "## Not yet": `Code signing / notarization, Windows ARM64, Linux.` and the Install
section sentence "There is no auto-updater yet: to update, install the newer build over the old
one." → "Later versions install themselves (see Updates)."

- [ ] **Step 3: CONTRIBUTING and CLAUDE.md**

`CONTRIBUTING.md` "## Releases", append a paragraph:

```markdown
Release builds also produce updater artifacts (`.sig` files and `latest.json`) signed with a
minisign key stored in the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the matching public key lives in
`src-tauri/tauri.conf.json` under `plugins.updater.pubkey`. Forks need their own keypair
(`pnpm tauri signer generate`) — and the maintainer's private key must never be lost: installed
apps only accept updates signed with it.
```

`CLAUDE.md`:
- Layout block: add `  src/update.rs           update check / install glue (tauri-plugin-updater)` after the `src/log.rs` line.
- Tech Stack plugin sentence: add `tauri-plugin-updater` (daily check, minisign-signed artifacts, feed = latest GitHub Release) to the list.
- Tauri Rules: replace `No auto-updater until asked.` at the end of the ad-hoc-signing bullet with `The updater verifies minisign signatures, so unsigned OS bundles are fine.`
- Status line → `**Status:** v1.7 (auto-update) implemented; released via the Changesets pipeline.` and add the spec path to the Specs list.

- [ ] **Step 4: Changeset, verify, commit, push, PR**

`.changeset/auto-update.md`:

```markdown
---
"claude-usage-monitor": minor
---

In-app updates: a daily check of the GitHub Releases feed, one notification per new version, and Help → Install update… to download, install and relaunch. Help → Check for updates… checks on demand.
```

Run `pnpm verify`, then:

```bash
git add .github/workflows/build-release.yml README.md CONTRIBUTING.md CLAUDE.md .changeset/auto-update.md
git commit -m "docs: updater documentation, release signing and changeset" -m "Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push -u origin feat/auto-update
gh pr create --base main --title "feat: auto-update" --body "$(cat <<'EOF'
## Summary
- `tauri-plugin-updater` (Rust only): daily background check (30 s after launch, then 24 h; skipped in debug builds), one notification per new version, Help → **Install update x.y.z…** downloads, installs and relaunches; Help → **Check for updates…** on demand
- Feed: `latest.json` on the latest GitHub Release, produced by `tauri-action` (`includeUpdaterJson`) and signed with the repo's minisign key; public key committed in `tauri.conf.json`
- Windows installs in NSIS passive mode; macOS swaps the bundle and restarts
- README/CONTRIBUTING/CLAUDE.md, minor changeset

## Test plan
- [x] `pnpm verify` green; `pnpm tauri dev` logs no update check (debug)
- [ ] First release with the updater (this PR's Version Packages): `latest.json` + `.sig` assets present on the release
- [ ] Manual: install that build on macOS and Windows; Help → Check for updates… → `Up to date`; ship the next patch; notification + Help → Install update…; app relaunches on the new version; log has `update installed …, restarting`
- [ ] Requires the repo to be public for the feed URL to resolve (until then: `Update check failed` on manual checks, silent in the background)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Report the PR URL.
