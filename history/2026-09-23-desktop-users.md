---
type: "docs"
status: "complete"
files:
  - src/App.tsx
  - README.md
  - .changeset/desktop-users.md
areas:
  - auth
  - docs
components:
  - popover
tags:
  - claude-desktop
  - onboarding
related-to: []
---

# docs: point Claude Desktop users to a Claude Code login

| Field       | Value                 |
| ----------- | --------------------- |
| **Status**  | complete              |
| **Branch**  | `docs/desktop-users`  |
| **Ticket**  | none                  |
| **Created** | 2026-09-23            |
| **Updated** | 2026-09-23            |

## Summary

The app reads only the Claude Code OAuth token, so a user who has only Claude Desktop sees
`◷ —` forever. Limits belong to the account, so a one-time `claude auth login` with the same
account gives the same numbers. The `no_token` banner now says so, the popover shows a
"Get Claude Code" link while no login is found, and the README tells Desktop-only users.

## Options considered

- **Paste the claude.ai `sessionKey`** into settings and poll
  `claude.ai/api/organizations/{org}/usage` (same JSON shape). Rejected for now: a second HTTP
  path, Cloudflare may block non-browser clients, the cookie expires, and it breaks the rule
  that credentials go only to `api.anthropic.com`.
- **Decrypt Claude Desktop's Electron cookie store** (`Claude Safe Storage` Keychain key /
  Windows DPAPI). Rejected: infostealer behaviour, Keychain prompt on every ad-hoc-signed
  update, Chromium app-bound encryption on Windows, breaks silently on format changes.

## Open question

Desktop bundles its own Claude Code (`~/Library/Application Support/Claude/claude-code`). If a
login from Desktop's Code tab writes the `Claude Code-credentials` Keychain entry, those users
already work with no extra step. Not verified.
