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
