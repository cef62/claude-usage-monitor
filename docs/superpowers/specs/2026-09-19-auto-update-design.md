# Auto-update (v1.7) — Design

Date: 2026-09-19. Status: approved.

## Goal

Let installed builds find, download and install newer releases from the GitHub Releases page
without the user visiting it: a daily background check, one notification per new version, and a
one-click "Install update" item in the tray menu. Builds stay unsigned by Apple/Microsoft; update
artifacts are minisign-signed so the app only installs what this repo's CI produced.

## Decisions

| Topic | Decision |
|---|---|
| Mechanism | `tauri-plugin-updater` 2.x, Rust side only (no npm package, no capability) |
| Feed | `https://github.com/cef62/claude-usage-monitor/releases/latest/download/latest.json`, produced by `tauri-action` (`includeUpdaterJson: true`) and merged across the macOS/Windows matrix jobs |
| Policy | Auto-check 30 s after startup and every 24 h; notify once per version; install only on click; manual "Check for updates…" |
| Keys | minisign keypair generated locally with `tauri signer generate --ci`; public key committed in `tauri.conf.json`; private key + password stored only in `~/.tauri/` (user backs them up) and in the repo secrets `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` |
| Debug builds | never check (`cfg!(debug_assertions)`), so `tauri dev` does not hit GitHub or try to replace itself; manual checks answer `Updates are disabled in debug builds` |
| Windows install | NSIS `installMode: "passive"` (progress bar, no questions); the installer relaunches the app |
| macOS install | plugin swaps the `.app` bundle from `.app.tar.gz`, then `app.restart()` |
| Failures | never surface a background failure (the repo is private until the user flips it, so 404s are expected for a while); log one line. Manual checks and installs report via notification |
| Settings | none (no opt-out toggle); the manual item is always available |

## Out of scope

Opt-out toggle, release notes in the notification, delta/differential updates, Windows ARM64,
Linux, updating *from* builds older than the first release that ships the updater (v0.7.x users
install the next version by hand once).

## Components

### `update.rs` (new)

```rust
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// What the last check found. `Update` is the plugin's handle (Clone), kept so "Install" needs
/// no second network round-trip to the feed.
#[derive(Default)]
pub struct UpdateState {
    pub available: Option<tauri_plugin_updater::Update>,
    pub notified: Option<String>,   // version already announced by notification
    pub busy: bool,                 // a check or install is running
}

/// Pure: announce a version once, never re-announce the same one.
pub fn should_notify(version: &str, notified: Option<&str>) -> bool;

/// Starts the background thread (release builds only): sleep FIRST_CHECK_DELAY, then loop
/// { check(app, Trigger::Auto); sleep CHECK_INTERVAL }.
pub fn spawn_checker(app: AppHandle);

pub enum Trigger { Auto, Manual }

/// Runs `app.updater_builder().configure_client(|c| c.read_timeout(READ_TIMEOUT)).build()?.check()`
/// on the async runtime (`tauri::async_runtime::block_on`), records the result in `UpdateState`,
/// updates the menu item, notifies per the policy, logs. `READ_TIMEOUT = 60s` is an idle
/// read timeout (idle-between-bytes), so a stalled socket can't pin `busy` until restart.
pub fn check(app: &AppHandle, trigger: Trigger);

/// Downloads and installs the stored `Update`, then `app.restart()`. Progress is logged every
/// 25 %. On error: notification "Update failed" + log; state cleared so the next check re-arms.
pub fn install(app: &AppHandle);
```

`check` behaviour:

| Result | Auto | Manual |
|---|---|---|
| newer version `v` | store `available`; menu item → `Install update v…` enabled; if `should_notify(v, notified)` → notification `Claude Usage Monitor v available` / `Right-click the tray icon → Help → Install update`, set `notified`; log `update available v` | same, but the notification is sent even if already notified |
| up to date | menu item → `Install update…` disabled; log `update none` | notification `Up to date (current)`; log |
| error | log `update check failed <err>` | notification `Update check failed` / `<err>`; log |

`busy` is checked-and-set under the lock before a check or install starts; a second request
while busy is dropped (manual: notification `Update in progress…`). The lock is never held across
the network call: take a snapshot, release, work, re-lock to store the result.

`install` after a successful `download_and_install`: log `update installed v, restarting`, then
`app.restart()`. On Windows the NSIS installer terminates the app itself; `restart()` is still
called and is harmless if the process is already exiting.

### Menu (`tray.rs`)

Help ▸ `Open log` · ─ · `Check for updates…` (id `check-updates`) · `Install update…` (id
`install-update`, `MenuItem`, disabled until an update is stored). Managed
`pub struct UpdateItem(pub MenuItem<Wry>)`. `MenuAction` gains `CheckUpdates` and
`InstallUpdate`; handlers call `update::check(app, Trigger::Manual)` / `update::install(app)`
on a spawned thread so the menu event loop returns immediately.

### `main.rs`

- `.plugin(tauri_plugin_updater::Builder::new().build())`
- `app.manage(Mutex::new(update::UpdateState::default()))`
- after `tray::setup`: `update::spawn_checker(app.handle().clone())`.

### `tauri.conf.json`

```json
"bundle": { "createUpdaterArtifacts": true, ... },
"plugins": {
  "updater": {
    "pubkey": "<content of ~/.tauri/claude-usage-monitor.key.pub>",
    "endpoints": ["https://github.com/cef62/claude-usage-monitor/releases/latest/download/latest.json"],
    "windows": { "installMode": "passive" }
  }
}
```

### CI (`build-release.yml`)

Matrix `tauri-action` step gains:

```yaml
env:
  GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
with:
  includeUpdaterJson: true
```

`tauri-action` uploads `latest.json` (+ `.sig` files) and merges the platform entries when the
second matrix job runs. With `createUpdaterArtifacts: true` every `tauri build` needs the private
key (it refuses to run with only a public key configured), so `ci.yml`'s two build steps get the
same two secrets in `env`. PRs from forks have no secret access and their build step fails —
accepted for a single-maintainer repo.

### Keys (one-time, controller runs it)

```bash
mkdir -p ~/.tauri && chmod 700 ~/.tauri
openssl rand -base64 24 > ~/.tauri/claude-usage-monitor.password && chmod 600 ~/.tauri/claude-usage-monitor.password
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat ~/.tauri/claude-usage-monitor.password)" \
  pnpm tauri signer generate --ci -w ~/.tauri/claude-usage-monitor.key >/dev/null
gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/claude-usage-monitor.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD < ~/.tauri/claude-usage-monitor.password
```

Only `claude-usage-monitor.key.pub` is read into the repo. The user backs up the two private
files; losing them means no installed build can ever accept an update again (a new keypair would
require a manual reinstall by every user).

### Docs and release

- README: "Updates" section (daily check, notification, Help items, first updater version must be
  installed by hand, works only once the repo is public). CONTRIBUTING: secrets list, key backup
  warning. CLAUDE.md: layout gains `src/update.rs`; Tauri Rules: updater plugin note; "No
  auto-updater until asked" line removed.
- Changeset `minor`.

## Testing

Rust, inline:

- `update::should_notify`: `("0.8.0", None) → true`, `("0.8.0", Some("0.8.0")) → false`,
  `("0.8.1", Some("0.8.0")) → true`.
- `tray::parse_menu_id`: `check-updates` → `CheckUpdates`, `install-update` → `InstallUpdate`.
- Constants: `FIRST_CHECK_DELAY < CHECK_INTERVAL` (const assert).

Manual (after the first release with the updater, say v0.8.0, and the repo public):

1. Install v0.8.0 on macOS and Windows.
2. Help → Check for updates… → `Up to date (0.8.0)`.
3. Ship v0.8.1 (any changeset). Within 24 h, or via the manual item: notification
   `Claude Usage Monitor 0.8.1 available`; Help shows `Install update 0.8.1…`.
4. Click it: macOS relaunches on 0.8.1 (menu bar title returns within seconds); Windows shows the
   NSIS progress bar, app relaunches. Log contains `update installed 0.8.1, restarting`.
5. `pnpm tauri dev` never logs an update check.

## Security notes

Update packages are verified against the committed public key before install; a tampered or
foreign `latest.json` is rejected by the plugin. The private key never enters the repo, the log,
or chat. No new capabilities; the only new network destinations are `github.com` (feed redirect),
`api.github.com` (asset URLs written by tauri-action into `latest.json`) and
`objects.githubusercontent.com` (asset download).
