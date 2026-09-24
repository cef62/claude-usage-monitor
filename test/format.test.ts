import { describe, expect, it } from 'vitest';
import {
  barColor,
  clock,
  countdown,
  elapsedPct,
  forecastText,
  levelColor,
  markClass,
  money,
  relative,
  sparkPoints,
} from '@/lib/format';
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

describe('levelColor', () => {
  it('turns amber at the lowest level and red at the highest', () => {
    expect(levelColor(79, [80, 95])).toBeNull();
    expect(levelColor(80, [80, 95])).toBe('warn');
    expect(levelColor(94.6, [80, 95])).toBe('warn');
    expect(levelColor(95, [80, 95])).toBe('over');
    expect(levelColor(130, [80, 95])).toBe('over');
  });

  it('goes straight to red with a single level and stays plain with none', () => {
    expect(levelColor(90, [95])).toBeNull();
    expect(levelColor(95, [95])).toBe('over');
    expect(levelColor(99, [])).toBeNull();
  });
});

describe('relative', () => {
  it('formats seconds and minutes ago', () => {
    expect(relative(3)).toBe('just now');
    expect(relative(42)).toBe('42s ago');
    expect(relative(180)).toBe('3m ago');
  });

  it('money formats minor units with the currency', () => {
    expect(money(1234, 'EUR', 2)).toBe('€12.34');
    expect(money(0, 'USD', 2)).toBe('$0.00');
    expect(money(500, 'JPY', 0)).toBe('¥500');
  });
});

describe('sparkPoints', () => {
  it('maps the window to the box and clamps', () => {
    const start = 1000;
    expect(
      sparkPoints(
        [
          { t: start, pct: 0 },
          { t: start + 500, pct: 150 },
          { t: start + 1000, pct: 100 },
        ],
        start,
        1000,
        100,
        28,
      ),
    ).toBe('0,28 50,0 100,0');
    expect(sparkPoints([{ t: start, pct: 10 }], start, 1000, 100, 28)).toBe('');
  });
});

describe('forecastText', () => {
  // The quota's window resets four days from NOW.
  const RESET = NOW + 4 * 86400;

  it('names the run-out time, with the weekday when not today', () => {
    expect(forecastText({ kind: 'runs_out', at: NOW + 2 * 3600 + 13 * 60 }, NOW, RESET)).toBe(
      'At this pace: 100% at 16:45',
    );
    expect(
      forecastText({ kind: 'runs_out', at: NOW + 3 * 86400 + 6 * 3600 + 28 * 60 }, NOW, RESET),
    ).toBe('At this pace: 100% at Sat 21:00');
  });

  it('rounds the projected percent at reset', () => {
    expect(forecastText({ kind: 'at_reset', percent: 77.6 }, NOW, RESET)).toBe(
      'At this pace: ~78% at reset',
    );
  });

  it('never rounds a projection up to 100% at reset', () => {
    // 100% is a run-out (red); a grey "~100% at reset" would contradict it.
    expect(forecastText({ kind: 'at_reset', percent: 99.6 }, NOW, RESET)).toBe(
      'At this pace: ~99% at reset',
    );
  });

  it('hides a run-out time that is already in the past', () => {
    // A snapshot from before a sleep: the popover must not claim "100% at 14:02" at 14:32.
    expect(forecastText({ kind: 'runs_out', at: NOW - 30 * 60 }, NOW, RESET)).toBeNull();
    expect(forecastText({ kind: 'runs_out', at: NOW }, NOW, RESET)).toBeNull();
  });

  it('hides the line once its window has reset', () => {
    // Polls failing past the reset: the forecast belongs to a window that is over.
    expect(forecastText({ kind: 'at_reset', percent: 78 }, NOW, NOW)).toBeNull();
    expect(forecastText({ kind: 'at_reset', percent: 78 }, NOW, NOW - 60)).toBeNull();
  });
});
