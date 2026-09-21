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

// Overage credits in the API's minor units; `decimals` says how to print them.
export type ExtraUsage = {
  used: number;
  limit: number | null;
  currency: string;
  decimals: number;
  utilization: number | null;
};

export type Snapshot = {
  quotas: Quota[];
  plan: string | null;
  extra: ExtraUsage | null;
  fetched_at: number | null;
  next_poll_at: number;
  status: Status;
};

export const SESSION_SECS = 5 * 3600;
export const WEEKLY_SECS = 7 * 86400;

export type PopoverSettings = {
  session_levels: number[];
  weekly_levels: number[];
  show_time_ticks: boolean;
  show_elapsed_marker: boolean;
  show_threshold_marks: boolean;
};

export const DEFAULT_POPOVER_SETTINGS: PopoverSettings = {
  session_levels: [80, 95],
  weekly_levels: [95],
  show_time_ticks: true,
  show_elapsed_marker: true,
  show_threshold_marks: true,
};
