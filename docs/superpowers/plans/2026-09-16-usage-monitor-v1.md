# Claude Usage Monitor v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A macOS menu bar app that shows Claude session/weekly usage percentages with reset countdowns, and opens a popover with bars, links, and status.

**Architecture:** A Rust poll thread reads the Claude Code OAuth token from the Keychain, calls `api.anthropic.com/api/oauth/usage`, normalizes the response into a `Snapshot`, updates the tray title, and emits the snapshot to a hidden frameless React window that is shown under the tray icon on click. React only renders.

**Tech Stack:** Tauri 2.11, Rust stable, reqwest 0.12 (rustls), serde/serde_json, React 19, TypeScript 7, Vite 8, Vitest 5, Biome 2.5, pnpm 12.

**Spec:** `docs/superpowers/specs/2026-09-16-usage-monitor-v1-design.md`

## Global Constraints

- No third-party UI libraries, icon packs, CSS frameworks or state libraries. React + plain CSS only.
- Biome (not ESLint/Prettier): 2-space indent, single quotes, trailing commas, semicolons always, `lineWidth` 100.
- All HTTP goes through Rust. The frontend never sees the OAuth token. Token is never logged or serialized.
- `#[tauri::command]` handlers return `Result<T, String>`. No `unwrap`/`expect` in `usage.rs`, `poll.rs`, `tray.rs` or command bodies.
- Headers on every usage request: `Authorization: Bearer <token>`, `anthropic-beta: oauth-2025-04-20`, `User-Agent: claude-code/<version>`, `Content-Type: application/json`.
- Poll timing: base 180s, cooldown 120s, error retry 30s, 429 backoff cap 900s, reset-aligned poll = seconds until nearest reset + 5.
- `tauri.conf.json` and `package.json` share version `0.1.0`. `Cargo.toml` version stays `0.0.0`.
- Capabilities: `core:default` and `opener:allow-open-url` scoped to `https://claude.ai/*`. Nothing else.
- Work happens on branch `feat/v1-app`, never on `main`.
- Commit messages: normal English, conventional prefix (`feat:`, `chore:`, `test:`, `docs:`), trailer `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`.

---

### Task 0: Toolchain and branch

**Files:**
- Create: `history/_draft-2026-09-16-v1-app.md` (via the history-driven-workflow skill)

**Interfaces:**
- Produces: a working `cargo`, `rustfmt`, `clippy`; branch `feat/v1-app`.

- [ ] **Step 1: Check what is installed**

Run: `xcode-select -p; node -v; pnpm -v; cargo --version; rustup --version`
Expected: Xcode CLT path and Node 24 / pnpm 12 print. `cargo` and `rustup` print `command not found` on this machine.

- [ ] **Step 2: Install Rust (only if Step 1 showed cargo missing)**

Ask the user to run this themselves (it writes to `~/.cargo` and `~/.rustup`):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile default
source "$HOME/.cargo/env"
```

`--profile default` includes `rustfmt` and `clippy`.

- [ ] **Step 3: Verify the toolchain**

Run: `source "$HOME/.cargo/env"; cargo --version && cargo fmt --version && cargo clippy --version`
Expected: three version lines, no errors. Every later `cargo` step in this plan assumes `~/.cargo/bin` is on `PATH`; if a step fails with `cargo: command not found`, prefix it with `source "$HOME/.cargo/env";`.

- [ ] **Step 4: Create the feature branch**

Run: `git checkout -b feat/v1-app`
Expected: `Switched to a new branch 'feat/v1-app'`

- [ ] **Step 5: Create the history record**

Invoke the `history-driven-workflow` skill to create `history/_draft-2026-09-16-v1-app.md` for this plan, pointing at the spec and this plan file. Commit it:

```bash
git add history
git commit -m "docs: add plan record for v1 app"
```

---

### Task 1: Frontend scaffold (Vite + React + Biome + Vitest)

**Files:**
- Create: `package.json`, `tsconfig.json`, `vite.config.ts`, `biome.json`, `index.html`, `src/main.tsx`, `src/App.tsx`, `src/app.css`, `test/smoke.test.ts`
- Modify: `.gitignore`

**Interfaces:**
- Produces: `pnpm dev`, `pnpm check`, `pnpm typecheck`, `pnpm test:run`, `pnpm build`; alias `@/*` → `src/*`.

- [ ] **Step 1: Write `package.json`**

```json
{
  "name": "claude-usage-monitor",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "packageManager": "pnpm@12.3.4",
  "engines": {
    "node": ">=24"
  },
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "tauri": "tauri",
    "check": "biome check .",
    "check:fix": "biome check --write .",
    "typecheck": "tsc --noEmit",
    "test": "vitest",
    "test:run": "vitest run",
    "verify": "pnpm check && pnpm typecheck && pnpm test:run && cargo fmt --manifest-path src-tauri/Cargo.toml --check && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings && cargo test --manifest-path src-tauri/Cargo.toml"
  },
  "dependencies": {
    "@tauri-apps/api": "2.11.1",
    "react": "19.3.0",
    "react-dom": "19.3.0"
  },
  "devDependencies": {
    "@biomejs/biome": "2.5.14",
    "@tauri-apps/cli": "2.11.4",
    "@types/react": "19.3.0",
    "@types/react-dom": "19.3.0",
    "@vitejs/plugin-react": "6.1.1",
    "typescript": "7.0.2",
    "vite": "8.3.0",
    "vitest": "5.0.1"
  }
}
```

- [ ] **Step 2: Write `tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2024",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "lib": ["ES2024", "DOM", "DOM.Iterable"],
    "types": ["vite/client"],
    "paths": {
      "@/*": ["./src/*"]
    }
  },
  "include": ["src", "test", "vite.config.ts"]
}
```

- [ ] **Step 3: Write `vite.config.ts`**

```ts
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: 'safari16' },
  test: { environment: 'node', include: ['test/**/*.test.ts'] },
});
```

- [ ] **Step 4: Write `biome.json`**

```json
{
  "$schema": "https://biomejs.dev/schemas/2.5.14/schema.json",
  "vcs": {
    "enabled": true,
    "clientKind": "git",
    "useIgnoreFile": true
  },
  "assist": { "actions": { "source": { "organizeImports": "on" } } },
  "formatter": {
    "indentStyle": "space",
    "indentWidth": 2,
    "lineWidth": 100,
    "lineEnding": "lf"
  },
  "javascript": {
    "formatter": {
      "quoteStyle": "single",
      "trailingCommas": "all",
      "semicolons": "always"
    }
  },
  "json": {
    "parser": { "allowComments": true }
  },
  "linter": {
    "enabled": true,
    "rules": {
      "recommended": true,
      "correctness": {
        "noUnusedImports": "error",
        "noUnusedVariables": "warn",
        "useExhaustiveDependencies": "warn"
      },
      "suspicious": {
        "noExplicitAny": "warn"
      },
      "style": {
        "noNonNullAssertion": "warn",
        "useConst": "error"
      }
    },
    "domains": {
      "react": "recommended"
    }
  },
  "files": {
    "includes": ["**", "!**/dist", "!**/src-tauri", "!**/docs", "!**/.claude", "!**/history"]
  }
}
```

- [ ] **Step 5: Write `index.html`**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Claude Usage Monitor</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 6: Write the minimal app**

`src/main.tsx`:

```tsx
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from '@/App';
import '@/app.css';

const root = document.getElementById('root');
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
```

`src/App.tsx`:

```tsx
export default function App() {
  return <div className="root">Claude Usage Monitor</div>;
}
```

`src/app.css`:

```css
html,
body {
  margin: 0;
  background: transparent;
}
```

- [ ] **Step 7: Write a smoke test**

`test/smoke.test.ts`:

```ts
import { expect, it } from 'vitest';

it('runs', () => {
  expect(1 + 1).toBe(2);
});
```

- [ ] **Step 8: Add `dist` note to `.gitignore`** — already present (`dist/`). Verify with `grep -n '^dist/' .gitignore`. Expected: one match.

- [ ] **Step 9: Install and verify**

Run: `pnpm install && pnpm check && pnpm typecheck && pnpm test:run && pnpm build`
Expected: install succeeds, Biome reports no errors, `tsc` exits 0, 1 test passes, `dist/index.html` exists. If Biome flags formatting, run `pnpm check:fix` once and re-run.

- [ ] **Step 10: Commit**

```bash
git add package.json pnpm-lock.yaml tsconfig.json vite.config.ts biome.json index.html src test
git commit -m "chore: scaffold Vite React frontend with Biome and Vitest"
```

---

### Task 2: Tauri scaffold (window, icons, capabilities, hello build)

**Files:**
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/Info.plist`, `src-tauri/capabilities/default.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/icons/tray.png`, `src-tauri/icons/app-icon.png` plus generated icons, `src-tauri/.gitignore`

**Interfaces:**
- Produces: `pnpm tauri dev` opens the app; a window labelled `popover` exists (hidden); crate name `claude_usage_monitor` with `lib.rs` module declarations.

- [ ] **Step 1: Write `src-tauri/Cargo.toml`**

```toml
[package]
name = "claude-usage-monitor"
# Deliberately not the product version. That lives in tauri.conf.json and package.json.
version = "0.0.0"
edition = "2021"
publish = false

[build-dependencies]
tauri-build = { version = "2.6", features = [] }

[dependencies]
tauri = { version = "2.11", features = ["tray-icon", "image-png"] }
tauri-plugin-opener = "2.5"
reqwest = { version = "0.12", default-features = false, features = ["blocking", "json", "rustls-tls-native-roots"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

- [ ] **Step 2: Write `src-tauri/build.rs`**

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 3: Write `src-tauri/src/lib.rs` and `src-tauri/src/main.rs`**

`lib.rs` (modules are added in later tasks; keep the file so `main.rs` can `use` the crate):

```rust
//! Library target so integration tests and the binary share the same modules.
```

`main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 4: Write `src-tauri/tauri.conf.json`**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Claude Usage Monitor",
  "version": "0.1.0",
  "identifier": "com.matteo.claude-usage-monitor",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:5173",
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build"
  },
  "app": {
    "macOSPrivateApi": true,
    "windows": [
      {
        "label": "popover",
        "title": "Claude Usage Monitor",
        "width": 320,
        "height": 240,
        "visible": false,
        "decorations": false,
        "transparent": true,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "resizable": false,
        "shadow": true
      }
    ],
    "security": {
      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'",
      "capabilities": ["default"]
    }
  },
  "bundle": {
    "active": true,
    "targets": ["app", "dmg", "nsis"],
    "macOS": {
      "minimumSystemVersion": "12.0"
    },
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

- [ ] **Step 5: Write `src-tauri/Info.plist`** (Tauri merges it into the bundle; hides the Dock icon)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>LSUIElement</key>
  <true/>
</dict>
</plist>
```

- [ ] **Step 6: Write `src-tauri/capabilities/default.json`**

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Popover window: core events plus opening claude.ai links in the browser",
  "windows": ["popover"],
  "permissions": [
    "core:default",
    {
      "identifier": "opener:allow-open-url",
      "allow": [{ "url": "https://claude.ai/*" }]
    }
  ]
}
```

- [ ] **Step 7: Write `src-tauri/.gitignore`**

```
target/
gen/schemas/
```

- [ ] **Step 8: Generate icons**

Write this one-off script to the scratchpad (not the repo) as `gen_icons.py` and run it with the icons directory as argument. It uses only the Python standard library.

```python
import math
import struct
import sys
import zlib


def png(path, size, pixel):
    rows = []
    for y in range(size):
        row = bytearray(b'\x00')
        for x in range(size):
            row += struct.pack('BBBB', *pixel(x, y, size))
        rows.append(bytes(row))
    raw = b''.join(rows)

    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data) & 0xFFFFFFFF)

    out = b'\x89PNG\r\n\x1a\n'
    out += chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0))
    out += chunk(b'IDAT', zlib.compress(raw))
    out += chunk(b'IEND', b'')
    with open(path, 'wb') as f:
        f.write(out)


def polar(x, y, size):
    c = (size - 1) / 2
    return math.hypot(x - c, y - c) / (size / 2), math.degrees(math.atan2(y - c, x - c))


def tray(x, y, size):
    # Black "C" ring on transparent background; macOS recolors template icons itself.
    r, ang = polar(x, y, size)
    inside = 0.5 <= r <= 0.9 and not (-35 < ang < 35)
    return (0, 0, 0, 255 if inside else 0)


def app(x, y, size):
    # White "C" ring on a terracotta disc.
    r, ang = polar(x, y, size)
    if r > 0.98:
        return (0, 0, 0, 0)
    if 0.45 <= r <= 0.75 and not (-35 < ang < 35):
        return (255, 255, 255, 255)
    return (217, 119, 87, 255)


out = sys.argv[1]
png(f'{out}/tray.png', 44, tray)
png(f'{out}/app-icon.png', 1024, app)
```

Run: `mkdir -p src-tauri/icons && python3 <scratchpad>/gen_icons.py src-tauri/icons && pnpm tauri icon src-tauri/icons/app-icon.png`
Expected: `src-tauri/icons/` contains `tray.png`, `app-icon.png`, `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico` and the Square*/StoreLogo files. The 1024px render takes a few seconds.

- [ ] **Step 9: Build and run once**

Run: `cd src-tauri && cargo build 2>&1 | tail -3; cd ..`
Expected: first build downloads crates (several minutes) and ends with `Finished`.

Run: `pnpm tauri dev` and leave it running for 20 seconds, then stop with Ctrl-C.
Expected: compiles, Vite starts on 5173, no Dock icon appears (Accessory policy), no window is visible (popover starts hidden), no panic in the terminal.

- [ ] **Step 10: Commit**

```bash
git add src-tauri
git commit -m "chore: scaffold Tauri shell with hidden popover window and capabilities"
```

---

### Task 3: `usage.rs` — credentials parsing, timestamp parsing, quota normalization

**Files:**
- Create: `src-tauri/src/usage.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct Credentials { pub token: String, pub fingerprint: u64 }`
  - `pub fn parse_credentials(blob: &str) -> Option<Credentials>`
  - `pub fn read_credentials() -> Option<Credentials>`
  - `pub fn parse_ts(s: &str) -> Option<i64>`
  - `#[derive(Debug, Clone, PartialEq, Serialize)] pub struct Quota { pub key: String, pub label: String, pub percent: f64, pub resets_at: i64, pub period_secs: u64 }`
  - `pub fn normalize(v: &serde_json::Value) -> Vec<Quota>`
  - `pub const SESSION_SECS: u64`, `pub const WEEKLY_SECS: u64`

- [ ] **Step 1: Declare the module**

`src-tauri/src/lib.rs`:

```rust
//! Library target so integration tests and the binary share the same modules.
pub mod usage;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/usage.rs` with only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
      "five_hour": {"utilization": 48.0, "resets_at": "2026-09-05T12:59:59.966454+00:00"},
      "seven_day": {"utilization": 64.0, "resets_at": "2026-09-11T20:59:59.966479+00:00"},
      "seven_day_sonnet": {"utilization": 2.0, "resets_at": "2026-09-11T20:59:59.966479+00:00"},
      "seven_day_opus": null,
      "nimbus_quill": {"utilization": 0.0, "resets_at": null},
      "limits": [
        {"kind": "session", "percent": 48, "resets_at": "2026-09-05T12:59:59.966454+00:00", "scope": null},
        {"kind": "weekly_all", "percent": 64, "resets_at": "2026-09-11T20:59:59.966479+00:00", "scope": null},
        {"kind": "weekly_scoped", "percent": 12, "resets_at": "2026-09-11T20:59:59.966479+00:00",
         "scope": {"model": {"display_name": "Fable"}}},
        {"kind": "weekly_scoped", "percent": null, "resets_at": "2026-09-11T20:59:59.966479+00:00",
         "scope": {"model": {"display_name": "Opus"}}}
      ]
    }"#;

    fn fixture() -> serde_json::Value {
        serde_json::from_str(FIXTURE).expect("fixture parses")
    }

    #[test]
    fn parse_ts_handles_offsets_and_fractions() {
        assert_eq!(parse_ts("2026-09-05T12:59:59.966454+00:00"), Some(1788613199));
        assert_eq!(parse_ts("2026-09-05T12:59:59Z"), Some(1788613199));
        assert_eq!(parse_ts("2026-09-05T14:59:59+02:00"), Some(1788613199));
        assert_eq!(parse_ts("garbage"), None);
        assert_eq!(parse_ts(""), None);
    }

    #[test]
    fn normalize_prefers_limits_and_drops_null_percent() {
        let q = normalize(&fixture());
        let keys: Vec<&str> = q.iter().map(|q| q.key.as_str()).collect();
        assert_eq!(keys, vec!["session", "weekly", "weekly:fable"]);
        assert_eq!(q[0].percent, 48.0);
        assert_eq!(q[0].resets_at, 1788613199);
        assert_eq!(q[0].period_secs, SESSION_SECS);
        assert_eq!(q[1].label, "Weekly");
        assert_eq!(q[1].period_secs, WEEKLY_SECS);
        assert_eq!(q[2].label, "Fable weekly");
        assert_eq!(q[2].percent, 12.0);
    }

    #[test]
    fn normalize_falls_back_to_flat_fields_without_limits() {
        let mut v = fixture();
        v.as_object_mut().expect("object").remove("limits");
        let q = normalize(&v);
        let keys: Vec<&str> = q.iter().map(|q| q.key.as_str()).collect();
        assert_eq!(keys, vec!["session", "weekly", "weekly:sonnet"]);
        assert_eq!(q[2].label, "Sonnet weekly");
    }

    #[test]
    fn normalize_falls_back_when_limits_is_empty() {
        let mut v = fixture();
        v["limits"] = serde_json::json!([]);
        assert_eq!(normalize(&v).len(), 3);
    }

    #[test]
    fn normalize_keeps_percent_above_100() {
        let mut v = fixture();
        v["limits"][0]["percent"] = serde_json::json!(112.5);
        assert_eq!(normalize(&v)[0].percent, 112.5);
    }

    #[test]
    fn normalize_returns_empty_for_non_object() {
        assert!(normalize(&serde_json::json!(null)).is_empty());
        assert!(normalize(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn parse_credentials_extracts_token_and_fingerprint() {
        let a = parse_credentials(r#"{"claudeAiOauth":{"accessToken":"sk-ant-abc","refreshToken":"r"}}"#)
            .expect("valid blob");
        assert_eq!(a.token, "sk-ant-abc");
        let b = parse_credentials(r#"{"claudeAiOauth":{"accessToken":"sk-ant-xyz","refreshToken":"r"}}"#)
            .expect("valid blob");
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn parse_credentials_rejects_bad_input() {
        assert!(parse_credentials("").is_none());
        assert!(parse_credentials("not json").is_none());
        assert!(parse_credentials(r#"{"claudeAiOauth":{}}"#).is_none());
        assert!(parse_credentials(r#"{"claudeAiOauth":{"accessToken":""}}"#).is_none());
        assert!(parse_credentials(r#"{"claudeAiOauth":null}"#).is_none());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml usage 2>&1 | grep -E 'error\[|cannot find' | head -5`
Expected: compile errors `cannot find function `parse_ts``, `normalize`, `parse_credentials`.

- [ ] **Step 4: Write the implementation** (above the `#[cfg(test)]` block)

```rust
//! Credentials, HTTP fetch and normalization of the Claude usage API.
//!
//! The token only ever lives in local variables here and in `poll.rs`. It is never
//! logged and never serialized.

use serde::Serialize;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub const SESSION_SECS: u64 = 5 * 3600;
pub const WEEKLY_SECS: u64 = 7 * 86400;

pub struct Credentials {
    pub token: String,
    /// Hash of the raw credential blob, so a 401 latch can notice a re-login without
    /// keeping the token around for comparison.
    pub fingerprint: u64,
}

pub fn parse_credentials(blob: &str) -> Option<Credentials> {
    let v: Value = serde_json::from_str(blob).ok()?;
    let token = v.get("claudeAiOauth")?.get("accessToken")?.as_str()?;
    if token.is_empty() {
        return None;
    }
    let mut h = DefaultHasher::new();
    blob.hash(&mut h);
    Some(Credentials {
        token: token.to_string(),
        fingerprint: h.finish(),
    })
}

pub fn read_credentials() -> Option<Credentials> {
    parse_credentials(&read_blob()?)
}

/// Claude Code on macOS stores the blob in the login Keychain under this service name.
#[cfg(target_os = "macos")]
fn read_blob() -> Option<String> {
    let out = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

#[cfg(not(target_os = "macos"))]
fn read_blob() -> Option<String> {
    let dir = std::env::var("CLAUDE_CONFIG_DIR").ok().or_else(|| {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()?;
        Some(format!("{home}/.claude"))
    })?;
    std::fs::read_to_string(format!("{dir}/.credentials.json")).ok()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Quota {
    /// `session`, `weekly`, or `weekly:<model>` (lowercase display name).
    pub key: String,
    pub label: String,
    /// Raw utilization; may exceed 100 during overage. The UI clamps for drawing only.
    pub percent: f64,
    /// Unix seconds.
    pub resets_at: i64,
    pub period_secs: u64,
}

/// Prefer `limits[]` (newer, carries per-model caps); fall back to the flat fields.
pub fn normalize(v: &Value) -> Vec<Quota> {
    let mut out: Vec<Quota> = v
        .get("limits")
        .and_then(Value::as_array)
        .map(|limits| limits.iter().filter_map(from_limit).collect())
        .unwrap_or_default();
    if out.is_empty() {
        out = from_flat(v);
    }
    out.sort_by(|a, b| rank(&a.key).cmp(&rank(&b.key)).then_with(|| a.key.cmp(&b.key)));
    out
}

fn rank(key: &str) -> u8 {
    match key {
        "session" => 0,
        "weekly" => 1,
        _ => 2,
    }
}

fn from_limit(l: &Value) -> Option<Quota> {
    let percent = l.get("percent")?.as_f64()?;
    let resets_at = parse_ts(l.get("resets_at")?.as_str()?)?;
    let (key, label, period_secs) = match l.get("kind")?.as_str()? {
        "session" => ("session".to_string(), "Session".to_string(), SESSION_SECS),
        "weekly_all" => ("weekly".to_string(), "Weekly".to_string(), WEEKLY_SECS),
        "weekly_scoped" => {
            let model = l.pointer("/scope/model/display_name")?.as_str()?;
            (
                format!("weekly:{}", model.to_lowercase()),
                format!("{model} weekly"),
                WEEKLY_SECS,
            )
        }
        _ => return None,
    };
    Some(Quota {
        key,
        label,
        percent,
        resets_at,
        period_secs,
    })
}

fn from_flat(v: &Value) -> Vec<Quota> {
    let Some(obj) = v.as_object() else {
        return Vec::new();
    };
    obj.iter()
        .filter_map(|(field, val)| {
            let (key, label, period_secs) = match field.as_str() {
                "five_hour" => ("session".to_string(), "Session".to_string(), SESSION_SECS),
                "seven_day" => ("weekly".to_string(), "Weekly".to_string(), WEEKLY_SECS),
                other => {
                    let name = other.strip_prefix("seven_day_")?;
                    (
                        format!("weekly:{name}"),
                        format!("{} weekly", capitalize(name)),
                        WEEKLY_SECS,
                    )
                }
            };
            let percent = val.get("utilization")?.as_f64()?;
            let resets_at = parse_ts(val.get("resets_at")?.as_str()?)?;
            Some(Quota {
                key,
                label,
                percent,
                resets_at,
                period_secs,
            })
        })
        .collect()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.frac](Z|±HH:MM)` into unix seconds. The API always sends
/// this shape, so a hand-written parser beats a chrono dependency.
pub fn parse_ts(s: &str) -> Option<i64> {
    let (date, rest) = s.trim().split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let tz_pos = rest.rfind(['Z', '+', '-'])?;
    let (time, tz) = rest.split_at(tz_pos);
    let time = time.split('.').next()?;
    let mut t = time.split(':').map(|p| p.parse::<i64>().ok());
    let (hh, mm, ss) = (t.next()??, t.next()??, t.next()??);
    let offset = if tz == "Z" {
        0
    } else {
        let sign = if tz.starts_with('-') { -1 } else { 1 };
        let (oh, om) = tz.get(1..)?.split_once(':')?;
        sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60)
    };
    Some(days_from_civil(y, m, day) * 86400 + hh * 3600 + mm * 60 + ss - offset)
}

/// Howard Hinnant's days-from-civil: days since 1970-01-01 for a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml usage`
Expected: `test result: ok. 8 passed`.

- [ ] **Step 6: Format**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml usage`
Expected: formatted, `8 passed`. Clippy with `-D warnings` first runs in Task 6, once the binary uses these functions (before that, rustc's `dead_code` warning would fail it).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/usage.rs
git commit -m "feat: parse Claude Code credentials and normalize usage quotas"
```

---

### Task 4: `usage.rs` — HTTP fetch and User-Agent

**Files:**
- Modify: `src-tauri/src/usage.rs`

**Interfaces:**
- Produces:
  - `#[derive(Debug, PartialEq)] pub enum FetchError { Unauthorized, RateLimited { retry_after: Option<u64> }, Server(u16), Network(String) }`
  - `pub const USAGE_BASE_URL: &str = "https://api.anthropic.com"`
  - `pub fn user_agent() -> String`
  - `pub fn client() -> reqwest::blocking::Client`
  - `pub fn fetch_usage(client: &reqwest::blocking::Client, base_url: &str, token: &str, user_agent: &str) -> Result<Value, FetchError>`
  - `pub fn retry_after_secs(header: Option<&str>) -> Option<u64>`

- [ ] **Step 1: Write the failing tests** (append inside the existing `mod tests`)

```rust
    #[test]
    fn retry_after_parses_seconds_only() {
        assert_eq!(retry_after_secs(Some("120")), Some(120));
        assert_eq!(retry_after_secs(Some(" 7 ")), Some(7));
        assert_eq!(retry_after_secs(Some("Wed, 21 Oct 2026 07:28:00 GMT")), None);
        assert_eq!(retry_after_secs(None), None);
    }

    #[test]
    fn user_agent_has_claude_code_prefix_and_numeric_version() {
        let ua = user_agent();
        let version = ua.strip_prefix("claude-code/").expect("prefix");
        assert!(version.chars().next().is_some_and(|c| c.is_ascii_digit()), "{ua}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml usage 2>&1 | grep -E 'cannot find' | head -3`
Expected: `cannot find function `retry_after_secs``, `user_agent`.

- [ ] **Step 3: Write the implementation** (add below `read_blob`, above `Quota`)

```rust
pub const USAGE_BASE_URL: &str = "https://api.anthropic.com";
/// Used when `claude --version` is unavailable. Any non-Claude-Code User-Agent has been
/// permanently rate limited upstream, so the prefix matters more than the exact number.
const FALLBACK_CLI_VERSION: &str = "2.1.273";

#[derive(Debug, PartialEq)]
pub enum FetchError {
    Unauthorized,
    RateLimited { retry_after: Option<u64> },
    Server(u16),
    Network(String),
}

pub fn user_agent() -> String {
    let version = std::process::Command::new("claude")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().next().map(str::to_string))
        .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or_else(|| FALLBACK_CLI_VERSION.to_string());
    format!("claude-code/{version}")
}

pub fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new())
}

/// Only the delta-seconds form is honored; an HTTP-date falls back to the caller's backoff.
pub fn retry_after_secs(header: Option<&str>) -> Option<u64> {
    header?.trim().parse().ok()
}

pub fn fetch_usage(
    client: &reqwest::blocking::Client,
    base_url: &str,
    token: &str,
    user_agent: &str,
) -> Result<Value, FetchError> {
    let resp = client
        .get(format!("{base_url}/api/oauth/usage"))
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Content-Type", "application/json")
        .header("User-Agent", user_agent)
        .send()
        .map_err(|e| FetchError::Network(e.without_url().to_string()))?;
    match resp.status().as_u16() {
        200..=299 => resp
            .json()
            .map_err(|e| FetchError::Network(e.without_url().to_string())),
        401 => Err(FetchError::Unauthorized),
        429 => {
            let header = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok());
            Err(FetchError::RateLimited {
                retry_after: retry_after_secs(header),
            })
        }
        code => Err(FetchError::Server(code)),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml usage`
Expected: `10 passed`.

- [ ] **Step 5: Lint and commit**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml usage`
Expected: formatted, `10 passed`.

```bash
git add src-tauri/src/usage.rs
git commit -m "feat: fetch usage from the Anthropic OAuth endpoint"
```

---

### Task 5: `poll.rs` — snapshot, delay policy, poll loop

**Files:**
- Create: `src-tauri/src/poll.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `usage::{read_credentials, fetch_usage, normalize, client, user_agent, FetchError, Quota, USAGE_BASE_URL}`
- Produces:
  - `#[derive(Debug, Clone, PartialEq, Serialize)] #[serde(tag = "kind", rename_all = "snake_case")] pub enum Status { Ok, NoToken, AuthExpired, RateLimited { until: i64 }, Error { message: String } }`
  - `#[derive(Debug, Clone, Serialize)] pub struct Snapshot { pub quotas: Vec<Quota>, pub fetched_at: Option<i64>, pub next_poll_at: i64, pub status: Status }` with `Default`
  - `pub type Shared = Arc<Mutex<Snapshot>>`
  - `pub fn read(shared: &Shared) -> Snapshot`
  - `pub fn now() -> i64`
  - `pub enum Outcome { Success, Unauthorized, RateLimited { retry_after: Option<u64> }, Failed }`
  - `pub fn next_delay(outcome: &Outcome, error_count: u32, nearest_reset: Option<i64>, now: i64) -> u64`
  - `pub fn run<F: Fn(&Snapshot) + Send + 'static>(shared: Shared, on_update: F)`
  - Constants `BASE_INTERVAL = 180`, `COOLDOWN = 120`, `ERROR_RETRY = 30`, `MAX_BACKOFF = 900`, `RESET_GRACE = 5`, `TITLE_TICK = 60`

- [ ] **Step 1: Declare the module**

`src-tauri/src/lib.rs`:

```rust
//! Library target so integration tests and the binary share the same modules.
pub mod poll;
pub mod usage;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/poll.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_789_588_800;

    #[test]
    fn success_uses_base_interval_without_imminent_reset() {
        assert_eq!(next_delay(&Outcome::Success, 0, None, NOW), BASE_INTERVAL);
        assert_eq!(next_delay(&Outcome::Success, 0, Some(NOW + 500), NOW), BASE_INTERVAL);
        assert_eq!(next_delay(&Outcome::Success, 0, Some(NOW - 10), NOW), BASE_INTERVAL);
    }

    #[test]
    fn success_aligns_to_an_imminent_reset() {
        assert_eq!(next_delay(&Outcome::Success, 0, Some(NOW + 60), NOW), 60 + RESET_GRACE);
    }

    #[test]
    fn failures_retry_quickly() {
        assert_eq!(next_delay(&Outcome::Failed, 3, None, NOW), ERROR_RETRY);
        assert_eq!(next_delay(&Outcome::Unauthorized, 0, None, NOW), ERROR_RETRY);
    }

    #[test]
    fn rate_limit_honors_retry_after_within_bounds() {
        let rl = |s| Outcome::RateLimited { retry_after: Some(s) };
        assert_eq!(next_delay(&rl(10), 1, None, NOW), BASE_INTERVAL);
        assert_eq!(next_delay(&rl(400), 1, None, NOW), 400);
        assert_eq!(next_delay(&rl(2000), 1, None, NOW), MAX_BACKOFF);
    }

    #[test]
    fn rate_limit_backs_off_exponentially_without_retry_after() {
        let rl = Outcome::RateLimited { retry_after: None };
        assert_eq!(next_delay(&rl, 1, None, NOW), 180);
        assert_eq!(next_delay(&rl, 2, None, NOW), 360);
        assert_eq!(next_delay(&rl, 3, None, NOW), 720);
        assert_eq!(next_delay(&rl, 4, None, NOW), MAX_BACKOFF);
        assert_eq!(next_delay(&rl, 40, None, NOW), MAX_BACKOFF);
    }

    #[test]
    fn status_serializes_with_kind_tag() {
        let ok = serde_json::to_value(Status::Ok).expect("serializes");
        assert_eq!(ok, serde_json::json!({"kind": "ok"}));
        let rl = serde_json::to_value(Status::RateLimited { until: 5 }).expect("serializes");
        assert_eq!(rl, serde_json::json!({"kind": "rate_limited", "until": 5}));
        let err = serde_json::to_value(Status::Error { message: "HTTP 500".into() }).expect("serializes");
        assert_eq!(err, serde_json::json!({"kind": "error", "message": "HTTP 500"}));
    }

    #[test]
    fn default_snapshot_has_no_token_and_no_quotas() {
        let s = Snapshot::default();
        assert_eq!(s.status, Status::NoToken);
        assert!(s.quotas.is_empty());
        assert_eq!(s.fetched_at, None);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml poll 2>&1 | grep -E 'cannot find|unresolved' | head -3`
Expected: `cannot find function `next_delay``, `cannot find type `Outcome``.

- [ ] **Step 4: Write the implementation** (above the tests)

```rust
//! Poll loop: owns the token for the duration of one request, applies the rate-limit
//! discipline, and publishes a `Snapshot` for the tray and the popover.

use crate::usage::{self, FetchError, Quota, USAGE_BASE_URL};
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const BASE_INTERVAL: u64 = 180;
pub const COOLDOWN: u64 = 120;
pub const ERROR_RETRY: u64 = 30;
pub const MAX_BACKOFF: u64 = 900;
pub const RESET_GRACE: u64 = 5;
/// Sleep slice; each slice re-renders the tray title so the countdown ticks.
pub const TITLE_TICK: u64 = 60;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Status {
    Ok,
    NoToken,
    AuthExpired,
    RateLimited { until: i64 },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    /// Last good quotas. An error never empties this.
    pub quotas: Vec<Quota>,
    pub fetched_at: Option<i64>,
    pub next_poll_at: i64,
    pub status: Status,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            quotas: Vec::new(),
            fetched_at: None,
            next_poll_at: 0,
            status: Status::NoToken,
        }
    }
}

pub type Shared = Arc<Mutex<Snapshot>>;

fn lock(shared: &Shared) -> MutexGuard<'_, Snapshot> {
    shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn read(shared: &Shared) -> Snapshot {
    lock(shared).clone()
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub enum Outcome {
    Success,
    Unauthorized,
    RateLimited { retry_after: Option<u64> },
    Failed,
}

pub fn next_delay(outcome: &Outcome, error_count: u32, nearest_reset: Option<i64>, now: i64) -> u64 {
    match outcome {
        Outcome::Success => match nearest_reset {
            Some(reset) if reset > now && (reset - now) as u64 + RESET_GRACE < BASE_INTERVAL => {
                (reset - now) as u64 + RESET_GRACE
            }
            _ => BASE_INTERVAL,
        },
        Outcome::Unauthorized | Outcome::Failed => ERROR_RETRY,
        Outcome::RateLimited {
            retry_after: Some(secs),
        } => (*secs).clamp(BASE_INTERVAL, MAX_BACKOFF),
        Outcome::RateLimited { retry_after: None } => {
            let shift = error_count.saturating_sub(1).min(4);
            (BASE_INTERVAL << shift).min(MAX_BACKOFF)
        }
    }
}

pub fn run<F: Fn(&Snapshot) + Send + 'static>(shared: Shared, on_update: F) {
    std::thread::spawn(move || {
        let client = usage::client();
        let user_agent = usage::user_agent();
        let mut error_count: u32 = 0;
        let mut last_success: Option<i64> = None;
        let mut latched_fingerprint: Option<u64> = None;

        loop {
            let now = now();
            // Clock-jump guard: a backwards jump must not freeze polling.
            if let Some(last) = last_success.filter(|last| now >= *last) {
                let since = (now - last) as u64;
                if since < COOLDOWN {
                    std::thread::sleep(Duration::from_secs(COOLDOWN - since));
                    continue;
                }
            }

            let outcome = match usage::read_credentials() {
                None => {
                    lock(&shared).status = Status::NoToken;
                    Outcome::Failed
                }
                Some(creds) if latched_fingerprint == Some(creds.fingerprint) => {
                    // Known-bad token: wait for the credential store to change.
                    Outcome::Unauthorized
                }
                Some(creds) => {
                    match usage::fetch_usage(&client, USAGE_BASE_URL, &creds.token, &user_agent) {
                        Ok(body) => {
                            let quotas = usage::normalize(&body);
                            error_count = 0;
                            latched_fingerprint = None;
                            last_success = Some(now);
                            let mut s = lock(&shared);
                            if !quotas.is_empty() {
                                s.quotas = quotas;
                            }
                            s.fetched_at = Some(now);
                            s.status = Status::Ok;
                            Outcome::Success
                        }
                        Err(FetchError::Unauthorized) => {
                            latched_fingerprint = Some(creds.fingerprint);
                            lock(&shared).status = Status::AuthExpired;
                            Outcome::Unauthorized
                        }
                        Err(FetchError::RateLimited { retry_after }) => {
                            error_count += 1;
                            Outcome::RateLimited { retry_after }
                        }
                        Err(FetchError::Server(code)) => {
                            error_count += 1;
                            lock(&shared).status = Status::Error {
                                message: format!("HTTP {code}"),
                            };
                            Outcome::Failed
                        }
                        Err(FetchError::Network(message)) => {
                            error_count += 1;
                            lock(&shared).status = Status::Error { message };
                            Outcome::Failed
                        }
                    }
                }
            };

            let nearest_reset = lock(&shared)
                .quotas
                .iter()
                .map(|q| q.resets_at)
                .filter(|r| *r > now)
                .min();
            let delay = next_delay(&outcome, error_count, nearest_reset, now);
            {
                let mut s = lock(&shared);
                if let Outcome::RateLimited { .. } = outcome {
                    s.status = Status::RateLimited {
                        until: now + delay as i64,
                    };
                }
                s.next_poll_at = now + delay as i64;
            }
            on_update(&read(&shared));

            let mut remaining = delay;
            while remaining > 0 {
                let step = remaining.min(TITLE_TICK);
                std::thread::sleep(Duration::from_secs(step));
                remaining -= step;
                on_update(&read(&shared));
            }
        }
    });
}
```

Note: `ponytail:` the cooldown check runs before the reset-aligned delay is honored, so a reset less than 120s after a successful fetch is observed on the next regular poll instead. Upgrade path: skip the cooldown when the previous outcome was a reset-aligned poll.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: `17 passed` (10 usage + 7 poll).

- [ ] **Step 6: Format, test, commit**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml`
Expected: formatted, `17 passed`.

```bash
git add src-tauri/src/lib.rs src-tauri/src/poll.rs
git commit -m "feat: add poll loop with cooldown, backoff and auth latch"
```

---

### Task 6: `tray.rs` + `main.rs` — tray title, popover toggle, commands

**Files:**
- Create: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `poll::{Snapshot, Status, Shared, read, now, run}`, `usage::Quota`
- Produces:
  - `pub const POPOVER_WIDTH: f64 = 320.0`, `pub const TRAY_ID: &str = "main"`
  - `pub fn countdown(secs: i64) -> String`
  - `pub fn title(s: &Snapshot, now: i64) -> String`
  - `pub fn popover_origin(rect: &tauri::Rect, scale: f64, width: f64) -> tauri::LogicalPosition<f64>`
  - `pub fn setup(app: &AppHandle) -> tauri::Result<()>`
  - `pub fn refresh_title(app: &AppHandle, s: &Snapshot)`
  - Commands `get_snapshot`, `hide_popover`, `resize_popover(height: u32)`, `quit`; event `usage` carrying `Snapshot`.

- [ ] **Step 1: Declare the module**

`src-tauri/src/lib.rs`:

```rust
//! Library target so integration tests and the binary share the same modules.
pub mod poll;
pub mod tray;
pub mod usage;
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/tray.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{Quota, SESSION_SECS, WEEKLY_SECS};
    use tauri::{LogicalSize, Position, Rect, Size};

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

    fn snapshot(status: Status, quotas: Vec<Quota>) -> Snapshot {
        Snapshot {
            quotas,
            fetched_at: Some(NOW),
            next_poll_at: NOW + 180,
            status,
        }
    }

    fn both() -> Vec<Quota> {
        vec![
            quota("session", 48.4, 2 * 3600 + 13 * 60, SESSION_SECS),
            quota("weekly", 64.0, 3 * 86400 + 4 * 3600 + 20 * 60, WEEKLY_SECS),
            quota("weekly:fable", 12.0, 3 * 86400, WEEKLY_SECS),
        ]
    }

    #[test]
    fn countdown_formats() {
        assert_eq!(countdown(30), "<1m");
        assert_eq!(countdown(-5), "<1m");
        assert_eq!(countdown(42 * 60), "42m");
        assert_eq!(countdown(65 * 60), "1h05m");
        assert_eq!(countdown(2 * 3600 + 13 * 60), "2h13m");
        assert_eq!(countdown(3 * 86400 + 4 * 3600 + 20 * 60), "3d4h");
    }

    #[test]
    fn title_ok_shows_both_halves_and_ignores_scoped() {
        let s = snapshot(Status::Ok, both());
        assert_eq!(title(&s, NOW), "⏱ 48% ↻2h13m · 📅 64% ↻3d4h");
    }

    #[test]
    fn title_missing_quota_shows_dash() {
        let s = snapshot(Status::Ok, vec![quota("session", 48.0, 600, SESSION_SECS)]);
        assert_eq!(title(&s, NOW), "⏱ 48% ↻10m · 📅 —");
    }

    #[test]
    fn title_by_status() {
        assert_eq!(title(&snapshot(Status::NoToken, vec![]), NOW), "⏱ —");
        assert_eq!(title(&snapshot(Status::AuthExpired, both()), NOW), "⏱ ! login");
        assert_eq!(
            title(&snapshot(Status::Error { message: "x".into() }, both()), NOW),
            "⏱ ! err"
        );
        assert_eq!(
            title(&snapshot(Status::RateLimited { until: NOW + 900 }, both()), NOW),
            "⏱ 48% ↻2h13m · 📅 64% ↻3d4h (429)"
        );
    }

    #[test]
    fn popover_is_centered_under_the_icon() {
        let rect = Rect {
            position: Position::Logical(LogicalPosition::new(1000.0, 0.0)),
            size: Size::Logical(LogicalSize::new(120.0, 22.0)),
        };
        let origin = popover_origin(&rect, 2.0, 320.0);
        assert_eq!(origin.x, 900.0);
        assert_eq!(origin.y, 28.0);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml tray 2>&1 | grep -E 'cannot find' | head -3`
Expected: `cannot find function `countdown``, `title`, `popover_origin`.

- [ ] **Step 4: Write the implementation** (above the tests)

```rust
//! Tray icon, its title text, the right-click menu, and popover placement.

use crate::poll::{self, Snapshot, Status};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, LogicalPosition, Manager, Rect};

pub const POPOVER_WIDTH: f64 = 320.0;
pub const TRAY_ID: &str = "main";
const USAGE_URL: &str = "https://claude.ai/settings/usage";
const POPOVER_GAP: f64 = 6.0;

pub fn countdown(secs: i64) -> String {
    if secs < 60 {
        return "<1m".to_string();
    }
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{mins:02}m")
    } else {
        format!("{mins}m")
    }
}

fn half(s: &Snapshot, key: &str, glyph: &str, now: i64) -> String {
    match s.quotas.iter().find(|q| q.key == key) {
        Some(q) => format!(
            "{glyph} {}% ↻{}",
            q.percent.round() as i64,
            countdown(q.resets_at - now)
        ),
        None => format!("{glyph} —"),
    }
}

pub fn title(s: &Snapshot, now: i64) -> String {
    let numbers = || format!("{} · {}", half(s, "session", "⏱", now), half(s, "weekly", "📅", now));
    match s.status {
        Status::Ok => numbers(),
        Status::RateLimited { .. } => format!("{} (429)", numbers()),
        Status::NoToken => "⏱ —".to_string(),
        Status::AuthExpired => "⏱ ! login".to_string(),
        Status::Error { .. } => "⏱ ! err".to_string(),
    }
}

/// Top-left corner for a popover of `width` logical pixels centered under the tray icon.
pub fn popover_origin(rect: &Rect, scale: f64, width: f64) -> LogicalPosition<f64> {
    let pos = rect.position.to_logical::<f64>(scale);
    let size = rect.size.to_logical::<f64>(scale);
    LogicalPosition::new(
        pos.x + size.width / 2.0 - width / 2.0,
        pos.y + size.height + POPOVER_GAP,
    )
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItemBuilder::with_id("open", "Open usage page").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app).item(&open).separator().item(&quit).build()?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .title("⏱ —")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => {
                let _ = tauri_plugin_opener::open_url(USAGE_URL, None::<&str>);
            }
            "quit" => app.exit(0),
            _ => {}
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

fn toggle_popover(app: &AppHandle, rect: &Rect) {
    let Some(window) = app.get_webview_window("popover") else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    let _ = window.set_position(popover_origin(rect, scale, POPOVER_WIDTH));
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn refresh_title(app: &AppHandle, s: &Snapshot) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(Some(title(s, poll::now())));
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: `22 passed`.

- [ ] **Step 6: Wire `main.rs`**

Replace `src-tauri/src/main.rs` entirely:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use claude_usage_monitor::poll::{self, Shared, Snapshot};
use claude_usage_monitor::tray;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

#[tauri::command]
fn get_snapshot(state: State<'_, Shared>) -> Result<Snapshot, String> {
    Ok(poll::read(&state))
}

#[tauri::command]
fn hide_popover(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("popover")
        .ok_or("no popover window")?
        .hide()
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn resize_popover(app: AppHandle, height: u32) -> Result<(), String> {
    app.get_webview_window("popover")
        .ok_or("no popover window")?
        .set_size(tauri::LogicalSize::new(tray::POPOVER_WIDTH, f64::from(height)))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn quit(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

fn main() {
    let shared: Shared = Arc::new(Mutex::new(Snapshot::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(shared.clone())
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            hide_popover,
            resize_popover,
            quit
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                let _ = window.hide();
            }
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::setup(app.handle())?;
            let handle = app.handle().clone();
            poll::run(shared, move |snapshot| {
                tray::refresh_title(&handle, snapshot);
                let _ = handle.emit("usage", snapshot);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 7: Lint**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: clean. If clippy reports an unused import or an API-name mismatch (for example `show_menu_on_left_click` or `tray_by_id`), fix by checking `docs.rs/tauri/2.11` — do not silence with `allow`.

- [ ] **Step 8: Manual check in the real app**

Run: `pnpm tauri dev`
Expected within ~3 minutes:
1. Menu bar shows `⏱ —` immediately, then real numbers like `⏱ 12% ↻3h41m · 📅 30% ↻4d2h` after the first poll (a few seconds). If the Keychain prompts for access to `Claude Code-credentials`, click **Always Allow**.
2. Left-click shows a 320×240 window under the icon with the placeholder text from Task 1; clicking elsewhere hides it. If the window appears at the bottom of the screen instead, macOS reported the rect with a flipped origin: change `popover_origin` to use `pos.y` only (`y = pos.y + POPOVER_GAP` when `pos.y < size.height`), update the test, and note it in the commit message.
3. Right-click shows `Open usage page` / `Quit`; `Open usage page` opens the browser; `Quit` exits.
4. Terminal shows no panic.

Stop with Ctrl-C.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src
git commit -m "feat: tray title with countdowns, popover toggle and app commands"
```

---

### Task 7: `src/lib/quota.ts` + `src/lib/format.ts` (pure, tested)

**Files:**
- Create: `src/lib/quota.ts`, `src/lib/format.ts`, `test/format.test.ts`
- Delete: `test/smoke.test.ts`

**Interfaces:**
- Produces:
  - `type Quota = { key: string; label: string; percent: number; resets_at: number; period_secs: number }`
  - `type Status = { kind: 'ok' } | { kind: 'no_token' } | { kind: 'auth_expired' } | { kind: 'rate_limited'; until: number } | { kind: 'error'; message: string }`
  - `type Snapshot = { quotas: Quota[]; fetched_at: number | null; next_poll_at: number; status: Status }`
  - `countdown(secs: number): string`, `clock(epoch: number, now: number): string`, `elapsedPct(q: Quota, now: number): number`, `barColor(percent: number, elapsed: number): 'ok' | 'warn' | 'over'`, `relative(secs: number): string`

- [ ] **Step 1: Write `src/lib/quota.ts`**

```ts
// Mirrors src-tauri/src/poll.rs. Field names are snake_case on both sides.
export type Quota = {
  key: string;
  label: string;
  percent: number;
  resets_at: number;
  period_secs: number;
};

export type Status =
  | { kind: 'ok' }
  | { kind: 'no_token' }
  | { kind: 'auth_expired' }
  | { kind: 'rate_limited'; until: number }
  | { kind: 'error'; message: string };

export type Snapshot = {
  quotas: Quota[];
  fetched_at: number | null;
  next_poll_at: number;
  status: Status;
};

export const SESSION_SECS = 5 * 3600;
export const WEEKLY_SECS = 7 * 86400;
```

- [ ] **Step 2: Write the failing tests**

`test/format.test.ts` (and delete `test/smoke.test.ts`):

```ts
import { describe, expect, it } from 'vitest';
import { barColor, clock, countdown, elapsedPct, relative } from '@/lib/format';
import { SESSION_SECS, WEEKLY_SECS } from '@/lib/quota';

// Wed 2026-09-16 14:32 local time.
const NOW = Math.floor(new Date(2026, 8, 16, 14, 32).getTime() / 1000);

describe('countdown', () => {
  it('formats minutes, hours and days', () => {
    expect(countdown(30)).toBe('<1m');
    expect(countdown(-5)).toBe('<1m');
    expect(countdown(42 * 60)).toBe('42m');
    expect(countdown(65 * 60)).toBe('1h05m');
    expect(countdown(2 * 3600 + 13 * 60)).toBe('2h13m');
    expect(countdown(3 * 86400 + 4 * 3600 + 20 * 60)).toBe('3d4h');
  });
});

describe('clock', () => {
  it('shows time only for today and weekday otherwise', () => {
    expect(clock(NOW + 2 * 3600 + 13 * 60, NOW)).toBe('16:45');
    expect(clock(NOW + 3 * 86400 + 6 * 3600 + 28 * 60, NOW)).toBe('Sat 21:00');
  });
});

describe('elapsedPct', () => {
  const session = { key: 'session', label: 'Session', percent: 0, resets_at: 0, period_secs: SESSION_SECS };
  it('is the elapsed fraction of the window', () => {
    expect(elapsedPct({ ...session, resets_at: NOW + 2 * 3600 + 13 * 60 }, NOW)).toBeCloseTo(55.67, 1);
  });
  it('clamps to 0..100', () => {
    expect(elapsedPct({ ...session, resets_at: NOW + 2 * SESSION_SECS }, NOW)).toBe(0);
    expect(elapsedPct({ ...session, resets_at: NOW - 10 }, NOW)).toBe(100);
    expect(elapsedPct({ ...session, period_secs: WEEKLY_SECS, resets_at: NOW + WEEKLY_SECS / 2 }, NOW)).toBe(50);
  });
});

describe('barColor', () => {
  it('is over when usage outpaces the clock or hits 100', () => {
    expect(barColor(48, 55.6)).toBe('ok');
    expect(barColor(60, 55.6)).toBe('over');
    expect(barColor(100, 100)).toBe('over');
    expect(barColor(112, 90)).toBe('over');
  });
  it('warns from 80', () => {
    expect(barColor(80, 90)).toBe('warn');
    expect(barColor(79.9, 90)).toBe('ok');
  });
});

describe('relative', () => {
  it('formats seconds and minutes ago', () => {
    expect(relative(3)).toBe('just now');
    expect(relative(42)).toBe('42s ago');
    expect(relative(180)).toBe('3m ago');
  });
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `rm test/smoke.test.ts && pnpm test:run 2>&1 | tail -5`
Expected: failure `Failed to resolve import "@/lib/format"`.

- [ ] **Step 4: Write `src/lib/format.ts`**

```ts
import type { Quota } from './quota';

const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

const pad = (n: number) => String(n).padStart(2, '0');

export function countdown(secs: number): string {
  if (secs < 60) return '<1m';
  const days = Math.floor(secs / 86400);
  const hours = Math.floor((secs % 86400) / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  if (days > 0) return `${days}d${hours}h`;
  if (hours > 0) return `${hours}h${pad(mins)}m`;
  return `${mins}m`;
}

export function clock(epoch: number, now: number): string {
  const t = new Date(epoch * 1000);
  const n = new Date(now * 1000);
  const hm = `${pad(t.getHours())}:${pad(t.getMinutes())}`;
  const sameDay =
    t.getFullYear() === n.getFullYear() &&
    t.getMonth() === n.getMonth() &&
    t.getDate() === n.getDate();
  return sameDay ? hm : `${DAYS[t.getDay()] ?? ''} ${hm}`;
}

export function elapsedPct(q: Quota, now: number): number {
  const elapsed = q.period_secs - (q.resets_at - now);
  return Math.min(100, Math.max(0, (elapsed / q.period_secs) * 100));
}

export type BarColor = 'ok' | 'warn' | 'over';

// "over" means usage is ahead of the clock: the bar fill has passed the elapsed-time marker.
export function barColor(percent: number, elapsed: number): BarColor {
  if (percent >= 100 || percent > elapsed) return 'over';
  return percent >= 80 ? 'warn' : 'ok';
}

export function relative(secs: number): string {
  if (secs < 5) return 'just now';
  if (secs < 60) return `${Math.floor(secs)}s ago`;
  return `${Math.floor(secs / 60)}m ago`;
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `pnpm test:run && pnpm check && pnpm typecheck`
Expected: `Tests 7 passed`, Biome clean, tsc clean.

- [ ] **Step 6: Commit**

```bash
git add src/lib test
git commit -m "feat: add snapshot types and pure formatting helpers"
```

---

### Task 8: `src/lib/ipc.ts` — the only IPC file, with browser fixture

**Files:**
- Create: `src/lib/ipc.ts`

**Interfaces:**
- Consumes: `Snapshot` from `@/lib/quota`; Rust commands `get_snapshot`, `hide_popover`, `resize_popover`, `quit`; event `usage`; plugin command `plugin:opener|open_url`.
- Produces: `getSnapshot(): Promise<Snapshot>`, `onUsage(cb): Promise<() => void>`, `hidePopover()`, `resizePopover(height: number)`, `openUrl(url: string)`, `quit()`, `FIXTURE: Snapshot`.

- [ ] **Step 1: Write `src/lib/ipc.ts`**

```ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Snapshot } from './quota';
import { SESSION_SECS, WEEKLY_SECS } from './quota';

// Outside Tauri (plain `pnpm dev` in a browser) every call is backed by this fixture so the
// UI can be iterated without the Rust shell.
const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

const nowSecs = Math.floor(Date.now() / 1000);
const weeklyReset = nowSecs + 3 * 86400 + 4 * 3600;

export const FIXTURE: Snapshot = {
  quotas: [
    {
      key: 'session',
      label: 'Session',
      percent: 48,
      resets_at: nowSecs + 2 * 3600 + 13 * 60,
      period_secs: SESSION_SECS,
    },
    { key: 'weekly', label: 'Weekly', percent: 64, resets_at: weeklyReset, period_secs: WEEKLY_SECS },
    {
      key: 'weekly:fable',
      label: 'Fable weekly',
      percent: 12,
      resets_at: weeklyReset,
      period_secs: WEEKLY_SECS,
    },
  ],
  fetched_at: nowSecs - 42,
  next_poll_at: nowSecs + 138,
  status: { kind: 'ok' },
};

export async function getSnapshot(): Promise<Snapshot> {
  return inTauri ? invoke<Snapshot>('get_snapshot') : FIXTURE;
}

export async function onUsage(cb: (s: Snapshot) => void): Promise<() => void> {
  if (!inTauri) return () => {};
  return listen<Snapshot>('usage', (event) => cb(event.payload));
}

export async function hidePopover(): Promise<void> {
  if (inTauri) await invoke('hide_popover');
}

export async function resizePopover(height: number): Promise<void> {
  if (inTauri) await invoke('resize_popover', { height: Math.ceil(height) });
}

export async function openUrl(url: string): Promise<void> {
  if (inTauri) await invoke('plugin:opener|open_url', { url });
  else window.open(url, '_blank', 'noopener');
}

export async function quit(): Promise<void> {
  if (inTauri) await invoke('quit');
}
```

- [ ] **Step 2: Verify**

Run: `pnpm check && pnpm typecheck`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/lib/ipc.ts
git commit -m "feat: add IPC wrappers with a browser fixture"
```

---

### Task 9: Popover UI — `App.tsx` + `app.css`

**Files:**
- Modify: `src/App.tsx`, `src/app.css`

**Interfaces:**
- Consumes: everything from `@/lib/format`, `@/lib/ipc`, `@/lib/quota`.
- Produces: the v1 popover.

- [ ] **Step 1: Write `src/App.tsx`**

```tsx
import { useEffect, useRef, useState } from 'react';
import { barColor, clock, countdown, elapsedPct, relative } from '@/lib/format';
import { getSnapshot, hidePopover, onUsage, openUrl, quit, resizePopover } from '@/lib/ipc';
import type { Quota, Snapshot, Status } from '@/lib/quota';
import { SESSION_SECS } from '@/lib/quota';

const USAGE_URL = 'https://claude.ai/settings/usage';
const BILLING_URL = 'https://claude.ai/settings/billing';
const STALE_GRACE = 30;

function useNow(): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(id);
  }, []);
  return now;
}

function statusText(status: Status, now: number): string | null {
  switch (status.kind) {
    case 'ok':
      return null;
    case 'no_token':
      return 'No Claude Code login found';
    case 'auth_expired':
      return 'Session expired — run claude auth login';
    case 'rate_limited':
      return `Rate limited, retrying at ${clock(status.until, now)}`;
    case 'error':
      return status.message;
  }
}

function QuotaCard({ q, now }: { q: Quota; now: number }) {
  const elapsed = elapsedPct(q, now);
  const color = barColor(q.percent, elapsed);
  const tickCount = q.period_secs === SESSION_SECS ? 5 : 7;
  const ticks = Array.from({ length: tickCount - 1 }, (_, i) => ((i + 1) / tickCount) * 100);
  return (
    <section className={`card ${color}`}>
      <header>
        <span className="label">{q.label}</span>
        <span className="pct">{Math.round(q.percent)}%</span>
      </header>
      <div className="bar">
        <div className="fill" style={{ width: `${Math.min(100, q.percent)}%` }} />
        {ticks.map((left) => (
          <i key={left} className="tick" style={{ left: `${left}%` }} />
        ))}
        <div className="marker" style={{ left: `${elapsed}%` }} />
      </div>
      <p className="reset">
        Resets in {countdown(q.resets_at - now)} · {clock(q.resets_at, now)}
      </p>
    </section>
  );
}

export default function App() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const now = useNow();
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let off = () => {};
    getSnapshot().then(setSnap);
    onUsage(setSnap).then((unlisten) => {
      off = unlisten;
    });
    return () => off();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') hidePopover();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const observer = new ResizeObserver(() => resizePopover(el.offsetHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  if (!snap) {
    return (
      <div ref={root} className="root">
        <p className="empty">Loading…</p>
      </div>
    );
  }

  const banner = statusText(snap.status, now);
  const stale = now > snap.next_poll_at + STALE_GRACE;

  return (
    <div ref={root} className={stale ? 'root stale' : 'root'}>
      {banner && <div className={`banner ${snap.status.kind}`}>{banner}</div>}
      {snap.quotas.length === 0 && !banner && <p className="empty">No usage data yet</p>}
      {snap.quotas.map((q) => (
        <QuotaCard key={q.key} q={q} now={now} />
      ))}
      <nav className="links">
        <button type="button" onClick={() => openUrl(USAGE_URL)}>
          Usage
        </button>
        <button type="button" onClick={() => openUrl(BILLING_URL)}>
          Billing
        </button>
        <button type="button" className="quit" onClick={() => quit()}>
          Quit
        </button>
      </nav>
      <footer>
        {snap.fetched_at === null ? 'Not fetched yet' : `Updated ${relative(now - snap.fetched_at)}`}
        {' · next '}
        {countdown(snap.next_poll_at - now)}
      </footer>
    </div>
  );
}
```

- [ ] **Step 2: Write `src/app.css`**

```css
:root {
  color-scheme: light dark;
  --bg: rgba(245, 245, 247, 0.92);
  --fg: #1d1d1f;
  --muted: #6e6e73;
  --track: rgba(0, 0, 0, 0.08);
  --line: rgba(0, 0, 0, 0.08);
  --ok: #34c759;
  --warn: #ff9f0a;
  --over: #ff3b30;
}

@media (prefers-color-scheme: dark) {
  :root {
    --bg: rgba(30, 30, 32, 0.92);
    --fg: #f5f5f7;
    --muted: #98989d;
    --track: rgba(255, 255, 255, 0.12);
    --line: rgba(255, 255, 255, 0.1);
  }
}

html,
body {
  margin: 0;
  background: transparent;
}

body {
  font: 13px/1.4 -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  color: var(--fg);
  -webkit-user-select: none;
  user-select: none;
}

.root {
  width: 320px;
  box-sizing: border-box;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: var(--bg);
  backdrop-filter: blur(20px);
  border-radius: 12px;
}

.root.stale .card {
  opacity: 0.6;
}

.banner {
  padding: 6px 8px;
  border-radius: 8px;
  background: var(--track);
  font-size: 12px;
}

.banner.auth_expired,
.banner.error {
  background: color-mix(in srgb, var(--over) 20%, transparent);
}

.empty {
  margin: 0;
  color: var(--muted);
  text-align: center;
}

.card header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
}

.card .label {
  font-weight: 600;
}

.card .pct {
  font-size: 20px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.bar {
  position: relative;
  height: 8px;
  margin: 6px 0 4px;
  border-radius: 4px;
  background: var(--track);
}

.fill {
  height: 100%;
  border-radius: 4px;
  background: var(--ok);
}

.card.warn .fill {
  background: var(--warn);
}

.card.over .fill {
  background: var(--over);
}

.tick {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--bg);
}

.marker {
  position: absolute;
  top: -2px;
  bottom: -2px;
  width: 2px;
  background: var(--fg);
  opacity: 0.7;
}

.reset {
  margin: 0;
  color: var(--muted);
  font-size: 12px;
}

.links {
  display: flex;
  gap: 6px;
  padding-top: 10px;
  border-top: 1px solid var(--line);
}

.links button {
  flex: 1;
  padding: 6px;
  border: 0;
  border-radius: 6px;
  background: var(--track);
  color: var(--fg);
  font: inherit;
  cursor: pointer;
}

.links button.quit {
  flex: 0 0 auto;
  color: var(--over);
}

footer {
  color: var(--muted);
  font-size: 11px;
  text-align: center;
}
```

- [ ] **Step 3: Lint and type-check**

Run: `pnpm check && pnpm typecheck && pnpm test:run`
Expected: clean. If Biome flags `noArrayIndexKey`, the ticks already key by percentage value — check the map uses `key={left}`.

- [ ] **Step 4: Check in the browser (fixture data)**

Run: `pnpm dev` and open `http://localhost:5173`.
Expected: three cards (Session 48%, Weekly 64%, Fable weekly 12%) with bars, a dark marker line partway along each bar, `Resets in 2h13m · HH:MM` lines, Usage/Billing/Quit buttons, footer `Updated 42s ago · next 2m`. Session bar is green (48 < ~55 elapsed). Toggle macOS dark mode: colours adapt. Stop the server.

- [ ] **Step 5: Check in the app (real data)**

Run: `pnpm tauri dev`
Expected: left-click the menu bar item → popover with real quotas, window height fits content (no empty space, no clipping), Esc hides it, click-outside hides it, `Usage` opens the browser and the popover hides (focus lost), `Quit` exits. Stop with Ctrl-C if still running.

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx src/app.css
git commit -m "feat: render usage bars, links and status in the popover"
```

---

### Task 10: Verify script, README, quality gate, finish branch

**Files:**
- Modify: `README.md`, `CLAUDE.md` (status line), `history/_draft-2026-09-16-v1-app.md` → renamed per skill

- [ ] **Step 1: Run the full verify**

Run: `pnpm verify`
Expected: Biome clean, tsc clean, `Tests 7 passed`, `cargo fmt --check` silent, clippy clean, `22 passed` Rust tests.

- [ ] **Step 2: Build the bundle once**

Run: `pnpm tauri build 2>&1 | tail -5`
Expected: ends with the `.app` and `.dmg` paths under `src-tauri/target/release/bundle/`. Open the `.app` once: menu bar item appears with real numbers (macOS may warn that the app is unsigned — right-click → Open).

- [ ] **Step 3: Update `README.md`**

Replace the file with:

```markdown
# Claude Usage Monitor

A macOS menu bar app that shows your Claude plan usage: percentage consumed in the 5-hour
session and weekly windows, and the countdown to each reset. Click it for a popover with
usage bars, an elapsed-time marker, and links to the claude.ai usage and billing pages.

Built with Tauri 2, Rust, React and TypeScript. No third-party UI libraries.

## How it works

The app reads the Claude Code OAuth token from the macOS Keychain (service
`Claude Code-credentials`), calls `https://api.anthropic.com/api/oauth/usage` every 3 minutes,
and shows the percentages the API reports. It never stores or logs the token. If you are not
logged in to Claude Code, or the token has expired, the menu bar shows `⏱ ! login`; run
`claude auth login` and the app recovers on its own.

Menu bar format: `⏱ 48% ↻2h13m · 📅 64% ↻3d4h` (session · weekly). ` (429)` after the text
means the API is rate limiting us and the numbers may be a few minutes old.

## Development

Requires Node 24+, pnpm 12, and a Rust stable toolchain (`rustup`).

```bash
pnpm install
pnpm tauri dev      # run the app
pnpm dev            # popover UI alone in a browser, with fixture data
pnpm verify         # biome, tsc, vitest, cargo fmt/clippy/test
pnpm tauri build    # .app and .dmg under src-tauri/target/release/bundle/
```

The app is unsigned. On first launch, right-click the `.app` and choose Open.

## Not yet

Threshold notifications, settings, Windows tray icon, launch at login, code signing.
```

- [ ] **Step 4: Update the status line in `CLAUDE.md`**

Change the `**Status (2026-09-16):**` paragraph to:

```markdown
**Status:** v1 implemented on branch `feat/v1-app` (menu bar text, popover, poll loop). Spec:
`docs/superpowers/specs/2026-09-16-usage-monitor-v1-design.md`.
```

- [ ] **Step 5: Run the quality gate**

Invoke the `quality-gate` skill. Expected table: C1 Verify PASS, C2 Tauri build PASS, C3 Docs PASS (README updated in Step 3), C4 Staleness CLEAN (no unused deps: `@tauri-apps/api`, `react`, `react-dom` are all imported; every Cargo dependency is used), C5 Capabilities PASS (`plugin:opener|open_url` ↔ `opener:allow-open-url`; app commands need no entry).

- [ ] **Step 6: Commit docs**

```bash
git add README.md CLAUDE.md
git commit -m "docs: describe v1 behavior, development commands and limits"
```

- [ ] **Step 7: Complete the history record and finish the branch**

Invoke the `history-driven-workflow` skill to complete the record (rename from `_draft-`), commit it with `docs: complete v1 app record`, then invoke `superpowers:finishing-a-development-branch` to merge or open the PR.
