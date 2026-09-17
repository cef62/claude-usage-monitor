# Menu Bar Settings and Title Polish (v1.1) — Design

Date: 2026-09-17. Status: approved.

## Goal

Let the user choose what the menu bar shows — session and/or weekly quota, per-quota glyph,
percent, remaining time — from the tray's right-click menu, persisted across launches. Replace the
colour 📅 emoji with monochrome glyphs and widen the spacing so the title reads cleanly next to the
template icon. Ship the macOS bundle ad-hoc signed so downloaded builds open (after the standard
"Open Anyway" step) instead of reporting "damaged".

## Decisions

| Topic | Decision |
|---|---|
| Settings surface | Tray right-click menu, submenu "Menu bar" with five check items. No popover changes. |
| Persistence | Rust-owned `Settings` struct, JSON at `app_data_dir/settings.json`, in Tauri managed state |
| "Icon" setting | Toggles the per-quota glyphs `◷` / `▦`; the tray template icon always stays |
| Glyphs | Session `◷` (U+25F7), weekly `▦` (U+25A6); no emoji |
| Spacing | Leading space, single spaces inside a half, `"  ·  "` between halves |
| Invariants | At least one of session/weekly and one of percent/remaining stays enabled |
| Bundle signing | `bundle.macOS.signingIdentity: "-"` (ad-hoc); no notarization yet |

## Out of scope

Popover settings panel, notifications, Windows tray, Developer ID signing and notarization
(separate task if an Apple Developer account exists), theme/colour options.

## Components

### `src-tauri/src/settings.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub session: bool,
    pub weekly: bool,
    pub glyph: bool,
    pub percent: bool,
    pub remaining: bool,
}
impl Default for Settings { /* all true */ }

pub const KEYS: [&str; 5] = ["session", "weekly", "glyph", "percent", "remaining"];

impl Settings {
    pub fn get(&self, key: &str) -> bool;
    /// Flips `key`. Returns false and leaves the struct unchanged when the flip would disable the
    /// last enabled member of a pair (session/weekly, percent/remaining) or the key is unknown.
    pub fn toggle(&mut self, key: &str) -> bool;
}

pub fn load(path: &Path) -> Settings;                 // missing or invalid file → Settings::default()
pub fn save(path: &Path, s: &Settings) -> io::Result<()>;  // creates parent dirs, pretty JSON + newline
pub fn path(app: &AppHandle) -> Option<PathBuf>;       // app_data_dir()/settings.json
```

`#[serde(default)]` means a file from an older or newer version still loads; unknown fields are
ignored.

### Title (`src-tauri/src/tray.rs`)

`pub fn title(s: &Snapshot, now: i64, settings: &Settings) -> String`.

- A half is rendered only if its setting is on. Parts in order: glyph (if `glyph`), `NN%` (if
  `percent`), `↻<countdown>` (if `remaining`), joined by one space. Missing quota with the half
  enabled → glyph (if on) + `—`.
- Enabled halves are joined by `"  ·  "`. The whole title starts with one space.
- Status strings: `Ok` → halves; `RateLimited` → halves + ` (429)`; `NoToken` → `◷ —`;
  `AuthExpired` → `◷ ! login`; `Error` → `◷ ! err`. With `glyph` off the `◷ ` prefix is dropped.
  These three also start with the leading space.
- Examples (all on): ` ◷ 13% ↻3h44m  ·  ▦ 9% ↻1d22h`; glyph off: ` 13% ↻3h44m  ·  9% ↻1d22h`;
  session only, percent off: ` ◷ ↻3h44m`.
- Per-model quotas never appear.

### Menu (`src-tauri/src/tray.rs`)

Right-click menu: `Open usage page` · submenu `Menu bar` [`Session`, `Weekly`, `Glyphs`,
`Percent`, `Remaining time`] · separator · `Quit`. Check items are `CheckMenuItem::with_id(app,
"set:<key>", label, true, checked, None)`; they are kept in a `MenuItems(HashMap<String,
CheckMenuItem<Wry>>)` in managed state so the handler can re-sync check marks.

Handler for `set:<key>`: lock `Mutex<Settings>`, `toggle(key)`; regardless of the result, call
`set_checked(settings.get(k))` on every item (so a refused toggle snaps the check mark back), save
the file (log nothing on failure — the in-memory value still applies), then `refresh_title`.

### Wiring (`src-tauri/src/main.rs`)

- `setup`: `let settings = settings::load(&settings::path(handle)?)`; `app.manage(Mutex::new(settings))`.
- `tray::setup(handle)` builds the menu from the current settings and manages `MenuItems`.
- `tray::refresh_title(app, snapshot)` reads `State<Mutex<Settings>>` and passes it to `title`.
- Poll closure unchanged.

### Signing (`src-tauri/tauri.conf.json`)

`"bundle": { "macOS": { "signingIdentity": "-" } }`. Tauri then `codesign`s the bundle ad-hoc so
Gatekeeper reports an unverified developer instead of a damaged app. README "Releases" gains the
exact steps: open once via System Settings → Privacy & Security → Open Anyway, or
`xattr -cr "/Applications/Claude Usage Monitor.app"`.

### Docs and release

- README: "Menu bar settings" paragraph under "How it works"; updated unsigned-app steps.
- CLAUDE.md: layout gains `src/settings.rs`; Data Source/title wording unchanged.
- Changeset `minor`: configurable menu bar, monochrome glyphs, spacing, ad-hoc signed bundle.

## Testing

Rust, inline `#[cfg(test)]`:

- `settings`: default all true; `toggle` flips independent keys; refuses turning off the last of
  each pair and returns `false`; unknown key → `false`; `load` of missing path and of garbage →
  default; `save` then `load` round-trip in a temp dir; JSON with an extra unknown field loads.
- `tray::title` matrix: all on; glyph off; session only; weekly only; percent off; remaining off;
  missing weekly quota with weekly on → ` ◷ 13% ↻3h44m  ·  ▦ —`; `NoToken` with glyph on/off;
  `RateLimited` suffix; scoped quota ignored.
- Menu behaviour is verified manually: toggle each item, restart the app, settings persist; try to
  uncheck both `Session` and `Weekly` — the second stays checked.

Signing verified on the next release asset: `codesign -dv --verbose=2 "Claude Usage Monitor.app"`
shows `Signature=adhoc` with a `CodeDirectory` covering resources, and `spctl` no longer reports
"code has no resources".

## Security notes

Settings file contains only five booleans. No new capabilities, no IPC additions.
