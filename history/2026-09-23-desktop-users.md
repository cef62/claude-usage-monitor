---
type: "docs"
status: "complete"
files:
  - src/App.tsx
  - README.md
  - CLAUDE.md
  - .changeset/desktop-users.md
areas:
  - auth
  - docs
  - policy
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
| **Updated** | 2026-09-24            |

## Summary

The app reads only the Claude Code OAuth token, so a user who has only Claude Desktop sees
`◷ —` forever. Limits belong to the account, so a one-time `claude auth login` with the same
account gives the same numbers. The `no_token` banner now says so, the popover shows a
"Get Claude Code" link while no login is found, and the README tells Desktop-only users.

## Options considered

- **Paste the claude.ai `sessionKey`** into settings and poll
  `claude.ai/api/organizations/{org}/usage` (same JSON shape). Ruled out: Anthropic's terms
  forbid third-party apps from storing claude.ai session tokens (see below). It would also add
  a second HTTP path, Cloudflare may block non-browser clients, and the cookie expires.
- **Decrypt Claude Desktop's Electron cookie store** (`Claude Safe Storage` Keychain key /
  Windows DPAPI). Rejected: infostealer behaviour, Keychain prompt on every ad-hoc-signed
  update, Chromium app-bound encryption on Windows, breaks silently on format changes.

- **An in-app OAuth login** (PKCE against claude.ai, own tokens in the OS keychain, refresh
  loop; about 2–3 days). Ruled out 2026-09-24: Anthropic registers no OAuth clients for third
  parties, so it would reuse Claude Code's client id, and the Claude Code legal page says
  "Anthropic does not permit third-party developers to offer Claude.ai login into their own
  applications [...] developers may not collect, store, or intermediate Claude.ai credentials
  or session tokens — sign-in to a Claude account must complete through Anthropic's own flow"
  (https://code.claude.com/docs/en/legal-and-compliance#authentication-and-credential-use).
  Server-side enforcement against consumer tokens outside Anthropic's apps started in
  January 2026.

## Known risk

The current design reads Claude Code's token and calls the usage endpoint with a
`claude-code/<version>` User-Agent. That is also a Claude.ai token used outside Anthropic's own
apps; Anthropic may enforce against it without notice (it would surface as a persistent `401` or
`429`). The README says so under "How it works". The only sanctioned route to an answer is
Anthropic sales/support.

## Verified: Desktop's Code tab is not enough

Desktop bundles its own Claude Code (`~/Library/Application Support/Claude/claude-code`).
Tested 2026-09-23: after `claude auth logout` the `Claude Code-credentials` Keychain entry was
gone, and signing in / using Desktop's Code tab did not recreate it. Desktop-only users need the
Claude Code CLI login; the banner and README guidance stand.
