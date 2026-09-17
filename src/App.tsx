import { useEffect, useRef, useState } from 'react';
import { barColor, clock, countdown, elapsedPct, relative } from '@/lib/format';
import { getSnapshot, hidePopover, onUsage, openUrl, quit, resizePopover } from '@/lib/ipc';
import type { Quota, Snapshot, Status } from '@/lib/quota';
import { SESSION_SECS } from '@/lib/quota';

const USAGE_URL = 'https://claude.ai/settings/usage';
const BILLING_URL = 'https://claude.ai/settings/billing';
const STALE_GRACE = 30;

function useNow(): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(id);
  }, []);
  return now;
}

function statusText(status: Status, now: number): string | null {
  switch (status.kind) {
    case 'ok':
      return null;
    case 'no_token':
      return 'No Claude Code login found';
    case 'auth_expired':
      return 'Session expired — run claude auth login';
    case 'rate_limited':
      return `Rate limited, retrying at ${clock(status.until, now)}`;
    case 'error':
      return status.message;
  }
}

function QuotaCard({ q, now }: { q: Quota; now: number }) {
  const elapsed = elapsedPct(q, now);
  const color = barColor(q.percent, elapsed);
  const tickCount = q.period_secs === SESSION_SECS ? 5 : 7;
  const ticks = Array.from({ length: tickCount - 1 }, (_, i) => ((i + 1) / tickCount) * 100);
  return (
    <section className={`card ${color}`}>
      <header>
        <span className="label">{q.label}</span>
        <span className="pct">{Math.round(q.percent)}%</span>
      </header>
      <div className="bar">
        <div className="fill" style={{ width: `${Math.min(100, q.percent)}%` }} />
        {ticks.map((left) => (
          <i key={left} className="tick" style={{ left: `${left}%` }} />
        ))}
        <div className="marker" style={{ left: `${elapsed}%` }} />
      </div>
      <p className="reset">
        Resets in {countdown(q.resets_at - now)} · {clock(q.resets_at, now)}
      </p>
    </section>
  );
}

export default function App() {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const now = useNow();
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    let off = () => {};
    getSnapshot().then((s) => {
      if (!cancelled) setSnap(s);
    });
    onUsage(setSnap).then((unlisten) => {
      if (cancelled) unlisten();
      else off = unlisten;
    });
    return () => {
      cancelled = true;
      off();
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') hidePopover();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const observer = new ResizeObserver(() => resizePopover(el.offsetHeight));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  if (!snap) {
    return (
      <div ref={root} className="root">
        <p className="empty">Loading…</p>
      </div>
    );
  }

  const banner = statusText(snap.status, now);
  const stale = now > snap.next_poll_at + STALE_GRACE;

  return (
    <div ref={root} className={stale ? 'root stale' : 'root'}>
      {banner && <div className={`banner ${snap.status.kind}`}>{banner}</div>}
      {snap.quotas.length === 0 && !banner && <p className="empty">No usage data yet</p>}
      {snap.quotas.map((q) => (
        <QuotaCard key={q.key} q={q} now={now} />
      ))}
      <nav className="links">
        <button type="button" onClick={() => openUrl(USAGE_URL)}>
          Usage
        </button>
        <button type="button" onClick={() => openUrl(BILLING_URL)}>
          Billing
        </button>
        <button type="button" className="quit" onClick={() => quit()}>
          Quit
        </button>
      </nav>
      <footer>
        {snap.fetched_at === null
          ? 'Not fetched yet'
          : `Updated ${relative(now - snap.fetched_at)}`}
        {' · next '}
        {countdown(snap.next_poll_at - now)}
      </footer>
    </div>
  );
}
