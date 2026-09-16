# Research: prior-art Claude usage monitors

Reviewed 2026-09-16. Both repos are MIT licensed.

- `jens-duttke/usage-monitor-for-claude` — Python 3.12 tray app (Windows/Linux), ~5.3k LOC.
- `claude-monitor/claude-monitor-browser-extension` — Manifest V3 extension, vanilla JS, ~4k LOC.

Neither reads local JSONL logs, `ccusage`, or statusline hooks. Both call an internal Anthropic
usage endpoint that returns **percentages**, not tokens. "Per-model" means per-model weekly
utilization %. Neither supports macOS.

## Data source (port from the Python app)

### Credentials

```
CLAUDE_CONFIG_DIR = $CLAUDE_CONFIG_DIR or ~/.claude
token = json.load(CLAUDE_CONFIG_DIR/.credentials.json)['claudeAiOauth']['accessToken']
```

On macOS Claude Code stores the same JSON blob in the Keychain instead, service
`Claude Code-credentials`. Verify locally:

```bash
security find-generic-password -s "Claude Code-credentials" -w
```

Read fresh on every poll: cheap, and it is how account switches and out-of-band token refreshes
get noticed. Read errors, a file mid-rewrite, or an empty `claudeAiOauth` after logout all mean
"no token right now", never a crash.

### Endpoints and headers

All GET, 10s timeout, only host `api.anthropic.com`.

```
GET https://api.anthropic.com/api/oauth/usage
GET https://api.anthropic.com/api/oauth/profile
GET https://api.anthropic.com/api/oauth/organizations/{org_uuid}/prepaid/credits   (only if extra_usage.is_enabled)

Authorization: Bearer <token>
Content-Type: application/json
User-Agent: claude-code/<installed cli version>     # fallback claude-code/2.1.204
anthropic-beta: oauth-2025-04-20
```

The User-Agent matters. The Python app once sent `usage-monitor-for-claude/x.y` and users were
permanently 429'd; the fix was `claude-code/<version>`, read from `claude --version` (cached by
binary mtime).

### `/api/oauth/usage` response (Sept 2026, anonymized)

```json
{
  "five_hour":  {"utilization": 48.0, "resets_at": "2026-09-05T12:59:59.966454+00:00",
                 "limit_dollars": null, "used_dollars": null, "remaining_dollars": null, "locked_reason": null},
  "seven_day":  {"utilization": 64.0, "resets_at": "2026-09-11T20:59:59.966479+00:00"},
  "seven_day_oauth_apps": null,
  "seven_day_opus": null,
  "seven_day_sonnet": {"utilization": 2.0, "resets_at": "..."},
  "seven_day_cowork": null, "seven_day_omelette": null,
  "tangelo": null, "iguana_necktie": null, "omelette_promotional": null,
  "nimbus_quill": {"utilization": 0.0, "resets_at": null},
  "cinder_cove": null, "copper_kite": null, "amber_ladder": null, "juniper_tide": null,
  "extra_usage": {"is_enabled": true, "monthly_limit": null, "used_credits": 0.0, "utilization": null,
                  "currency": "EUR", "decimal_places": 2, "disabled_reason": null, "user_disabled": false,
                  "spend_limit_reached": false, "credits_ever_enabled": true, "daily": null, "weekly": null},
  "limits": [
    {"kind": "session",    "group": "session", "percent": 48, "severity": "normal", "resets_at": "...", "scope": null, "is_active": true},
    {"kind": "weekly_all", "group": "weekly",  "percent": 64, "severity": "normal", "resets_at": "...", "scope": null, "is_active": false},
    {"kind": "weekly_scoped", "group": "weekly", "percent": 12, "resets_at": "...", "scope": {"model": {"display_name": "Fable"}}}
  ],
  "spend": {"used": {"amount_minor": 0, "currency": "EUR", "exponent": 2}, "limit": null, "percent": 0,
            "severity": "normal", "enabled": true, "balance": null, "can_purchase_credits": false},
  "member_dashboard_available": false
}
```

Facts:

- `utilization` is a percentage and may exceed 100 with overage. Clamp for display only.
- `resets_at` is ISO-8601 with microseconds and has sub-second jitter between polls. Never compare
  for equality; the extension uses `after > before && before <= now` to detect a rollover.
- Newer per-model weekly caps exist only in `limits[]` with `kind == "weekly_scoped"`; the flat
  `seven_day_opus` / `seven_day_sonnet` fields may be null. Parse both, prefer `limits[]`.
- Code-named fields (`nimbus_quill`, ...) appear before they apply. A quota is active only if it
  has a `resets_at`.
- Money is in minor units (cents). `extra_usage.monthly_limit == null` means uncapped overage.

### `/api/oauth/profile`

```json
{"account": {"uuid", "full_name", "display_name", "email", "has_claude_max", "has_claude_pro", "created_at"},
 "organization": {"uuid", "name", "organization_type": "claude_max", "billing_type",
                  "rate_limit_tier": "default_claude_max_5x", "has_extra_usage_enabled",
                  "subscription_status", "subscription_created_at"},
 "application": {"uuid", "name": "Claude Code", "slug": "claude-code"}}
```

Plan label: `organization_type.replace('_', ' ').title()` gives "Claude Max"; `rate_limit_tier`
carries the 5x/20x detail (the extension maps `max_20x` → "Max 20x", `max_5x` → "Max 5x", then
team / enterprise / pro / free).

### Error mapping

401 → auth error; 429 → rate limited, parse `Retry-After`; 5xx → server error; TLS error →
certificate error (corporate proxies; use the OS trust store). The server's `error.message` from
the JSON body is surfaced in the UI.

### Token refresh

On 401 the Python app runs `claude update` hoping the CLI rewrites the token as a side effect.
Issue #89 shows this is unreliable on the native installer. Do not copy it. Instead: stop using the
known-bad token, keep re-reading the credential store each poll, resume when the bytes change, and
tell the user to run `claude auth login`.

### Polling (seconds)

```
poll_interval   = 180   normal cadence (originally 120, raised because of 429s)
poll_fast       = 120   cooldown: never two successful fetches closer than this
poll_error      = 30    retry after a non-429 error
max_backoff     = 900   cap for 429 exponential backoff (interval * 2^(errors-1), or Retry-After clamped to [180, 900])
idle_pause      = 300   user idle after 5 min drops cadence to
idle_interval   = 900   15 min while idle/locked (polling never fully stops)
```

Also worth copying:

- `_align_to_reset()` shifts the next poll so one lands a few seconds after an imminent
  `resets_at`, giving instant "session reset" feedback without polling faster.
- Clock-jump guards: if `now < last_success` or backoff exceeds the cap, clamp.
- One lock plus cooldown so popup-open and the poll loop never double-fetch. The "Refresh now"
  menu item was removed for the same reason. Treat 2–3 min as the safe floor per token.
- The profile fetch respects the 429 backoff too.

## Metrics

- One bar per active quota: `five_hour`, `seven_day`, `seven_day_<model>`, plus any
  `limits[].scope.model` entry.
- Per bar: utilization %, reset countdown and clock time, and an **elapsed-time marker**:
  `elapsed = period - (resets_at - now)`, `elapsed_pct = clamp(elapsed / period * 100)`. The bar
  turns red when `utilization >= 100 || utilization > elapsed_pct` (usage outpacing the clock).
- Dividers: the 5h bar split into 5 hour ticks; the weekly bar split at local midnights.
- Extra usage: `used_credits / monthly_limit` (%), formatted with currency and `decimal_places`.
- No token counts, no cost estimates, in either repo.

The Python `formatting.py` (`parse_field_name`, `field_period`, `is_active_quota`,
`elapsed_pct`, `divider_positions`, `time_until`, `format_credits`) is pure functions and ports
straight to TypeScript. Its `tests/` directory doubles as an edge-case spec.

## UI / UX ideas

Python tray app:

- Tray icon: letter "C" plus two stacked mini bars (session/weekly) with time markers, or two
  stacked percentages; theme-aware for light/dark; "C!" = expired session, "!" = error,
  "✕" = depleted.
- Left-click opens a frameless HTML popup: account section, usage bars, extra usage, status footer
  with a live "updated Xs ago / next poll in" ticker. Pin button and drag. The popup reports its
  content height via `ResizeObserver` so the native window auto-sizes (maps to Tauri
  `window.setSize`). Data dims as stale when `now > next_poll_time + 30s`.
- Notifications: thresholds per quota, defaults `five_hour: [50, 80, 95]`, `seven_day: [95]`,
  `extra_usage: [50, 80, 95]`; each fires once, re-arms when usage drops. Time-aware mode
  (default on): below 90% a threshold only alerts if `utilization > elapsed_pct`. A reset
  notification fires when a quota that was >95% (5h) / >98% (7d) drops. Notifications are deferred
  while the user is idle/locked and flushed on return.

Browser extension:

- Badge shows session %; colour green <50, amber 50–80, red ≥80.
- Popup cards per quota with % , bar or ring gauge (`stroke-dasharray "${pct} 100"` with
  `pathLength=100`), "Resets in Xd Yh / Xh Ym / Xm" plus absolute time, and an SVG sparkline
  drawn over the live reset window (x-axis = window start → `resets_at`, so a curve never crosses a
  reset). `popup.js` lines 442–660 are a self-contained ~200 LOC sparkline.
- Auth expired: inline banner and dimmed cards; last data stays visible, never replaced by zeros.
- Notifications: `warnAt: 80`, `critAt: 95`; state keyed by bucket + `resetTime` as the window id,
  so each threshold fires once per reset window and crit suppresses a late warn.
- Usage history: one sample per successful refresh, min 10-minute gap unless the reset window
  rolled over, max 5000 samples / 30 days, JSON/CSV export. Failed refreshes write nothing.

## Browser-extension data path (skip)

Cookie-authenticated `https://claude.ai/api/organizations/{org}/usage` and friends. Browser-only,
same JSON shape as the OAuth endpoint. Useful only as a reference for `limits[]` normalization
(`background.js` `mapLimitsArray`).

## Borrow list

Must-have:

1. Keychain/file token read → `/api/oauth/usage` in Rust (`reqwest` + `serde_json::Value`).
2. Rate-limit state machine (lock, cooldown, backoff, 401 latch).
3. `limits[]`-first quota normalization.
4. Per-bar metrics with elapsed-time marker and "ahead of clock" colour.
5. Tray/menu-bar presence with session % and reset time; frameless auto-sizing window.
6. Auth-expired state that keeps the last data visible.

Nice-to-have:

7. Threshold notifications, once per reset window, time-aware suppression.
8. Reset-aligned polling and idle slowdown.
9. Local history and sparkline over the live window.
10. Plan badge from profile `rate_limit_tier`; extra-usage / prepaid credits card.
11. Stale-data dimming when a poll is overdue.

Skip: `claude update` token refresh, claude.ai cookie endpoints, locales/themes/layouts, shell
event hooks, installed-version section, JSONL token counting, promo/uninstall code.
