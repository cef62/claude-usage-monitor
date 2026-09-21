---
"claude-usage-monitor": patch
---

settings.json and history.json are written atomically (temp file + rename), so a crash mid-write can no longer reset settings or the history. Dependencies refreshed (Tauri 2.11.6, updater plugin 2.12); Dependabot keeps actions and crates current.
