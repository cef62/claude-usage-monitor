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

export type Sample = { t: number; pct: number };

// Mirrors src-tauri/src/forecast.rs: where usage lands at the recent pace.
export type Forecast = { kind: 'runs_out'; at: number } | { kind: 'at_reset'; percent: number };

export type Snapshot = {
  quotas: Quota[];
  plan: string | null;
  extra: ExtraUsage | null;
  fetched_at: number | null;
  next_poll_at: number;
  status: Status;
  history: Record<string, Sample[]>;
  forecast: Record<string, Forecast>;
};

export const SESSION_SECS = 5 * 3600;
export const WEEKLY_SECS = 7 * 86400;

export type PopoverSettings = {
  session_levels: number[];
  weekly_levels: number[];
  show_time_ticks: boolean;
  show_elapsed_marker: boolean;
  show_threshold_marks: boolean;
  show_history: boolean;
  show_forecast: boolean;
  color_percent: boolean;
};

export const DEFAULT_POPOVER_SETTINGS: PopoverSettings = {
  session_levels: [80, 95],
  weekly_levels: [95],
  show_time_ticks: true,
  show_elapsed_marker: true,
  show_threshold_marks: true,
  show_history: true,
  show_forecast: true,
  color_percent: true,
};
