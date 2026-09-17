import { describe, expect, it } from 'vitest';
import { barColor, clock, countdown, elapsedPct, markClass, relative } from '@/lib/format';
import { SESSION_SECS, WEEKLY_SECS } from '@/lib/quota';

// Wed 2026-09-16 14:32 local time.
const NOW = Math.floor(new Date(2026, 8, 16, 14, 32).getTime() / 1000);

describe('countdown', () => {
  it('formats minutes, hours and days', () => {
    expect(countdown(30)).toBe('<1m');
    expect(countdown(-5)).toBe('<1m');
    expect(countdown(42 * 60)).toBe('42m');
    expect(countdown(65 * 60)).toBe('1h05m');
    expect(countdown(2 * 3600 + 13 * 60)).toBe('2h13m');
    expect(countdown(3 * 86400 + 4 * 3600 + 20 * 60)).toBe('3d4h');
  });
});

describe('clock', () => {
  it('shows time only for today and weekday otherwise', () => {
    expect(clock(NOW + 2 * 3600 + 13 * 60, NOW)).toBe('16:45');
    expect(clock(NOW + 3 * 86400 + 6 * 3600 + 28 * 60, NOW)).toBe('Sat 21:00');
  });
});

describe('elapsedPct', () => {
  const session = {
    key: 'session',
    label: 'Session',
    percent: 0,
    resets_at: 0,
    period_secs: SESSION_SECS,
  };
  it('is the elapsed fraction of the window', () => {
    expect(elapsedPct({ ...session, resets_at: NOW + 2 * 3600 + 13 * 60 }, NOW)).toBeCloseTo(
      55.67,
      1,
    );
  });
  it('clamps to 0..100', () => {
    expect(elapsedPct({ ...session, resets_at: NOW + 2 * SESSION_SECS }, NOW)).toBe(0);
    expect(elapsedPct({ ...session, resets_at: NOW - 10 }, NOW)).toBe(100);
    expect(
      elapsedPct({ ...session, period_secs: WEEKLY_SECS, resets_at: NOW + WEEKLY_SECS / 2 }, NOW),
    ).toBe(50);
  });
});

describe('barColor', () => {
  it('is over when usage outpaces the clock or hits 100', () => {
    expect(barColor(48, 55.6)).toBe('ok');
    expect(barColor(60, 55.6)).toBe('over');
    expect(barColor(100, 100)).toBe('over');
    expect(barColor(112, 90)).toBe('over');
  });
  it('warns from 80', () => {
    expect(barColor(80, 90)).toBe('warn');
    expect(barColor(79.9, 90)).toBe('ok');
  });
});

describe('markClass', () => {
  it('colours the top level red and the rest amber', () => {
    expect(markClass(95, [80, 95])).toBe('over');
    expect(markClass(80, [80, 95])).toBe('warn');
    expect(markClass(95, [95])).toBe('over');
  });
});

describe('relative', () => {
  it('formats seconds and minutes ago', () => {
    expect(relative(3)).toBe('just now');
    expect(relative(42)).toBe('42s ago');
    expect(relative(180)).toBe('3m ago');
  });
});
