---
"claude-usage-monitor": patch
---

Release workflow creates the GitHub Release once and builds macOS and Windows in parallel, so one platform failing no longer blocks the other's assets. CI actions bumped off the deprecated Node 20 runtime.
