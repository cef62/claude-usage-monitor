import type { ExtraUsage, Quota } from './quota';

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

export function markClass(level: number, levels: number[]): BarColor {
  return level >= Math.max(...levels) ? 'over' : 'warn';
}

export function relative(secs: number): string {
  if (secs < 5) return 'just now';
  if (secs < 60) return `${Math.floor(secs)}s ago`;
  return `${Math.floor(secs / 60)}m ago`;
}

export function money(minor: number, currency: string, decimals: number): string {
  const amount = minor / 10 ** decimals;
  try {
    return new Intl.NumberFormat(undefined, {
      style: 'currency',
      currency,
      minimumFractionDigits: decimals,
      maximumFractionDigits: decimals,
    }).format(amount);
  } catch {
    return `${amount.toFixed(decimals)} ${currency}`;
  }
}

// Extra usage has no clock to be ahead of: only the plain thresholds apply.
export function extraPct(e: ExtraUsage): number | null {
  if (e.utilization !== null) return e.utilization;
  if (e.limit !== null && e.limit > 0) return (e.used / e.limit) * 100;
  return null;
}
