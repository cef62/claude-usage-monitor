import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { PopoverSettings, Snapshot } from './quota';
import { DEFAULT_POPOVER_SETTINGS, SESSION_SECS, WEEKLY_SECS } from './quota';

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
    {
      key: 'weekly',
      label: 'Weekly',
      percent: 64,
      resets_at: weeklyReset,
      period_secs: WEEKLY_SECS,
    },
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

export async function getSettings(): Promise<PopoverSettings> {
  return inTauri ? invoke<PopoverSettings>('get_settings') : DEFAULT_POPOVER_SETTINGS;
}

export async function onSettings(cb: (s: PopoverSettings) => void): Promise<() => void> {
  if (!inTauri) return () => {};
  return listen<PopoverSettings>('settings', (event) => cb(event.payload));
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
