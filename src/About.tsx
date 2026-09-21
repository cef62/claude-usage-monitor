import type { About as AboutInfo } from '@/lib/about';
import { aboutLinks } from '@/lib/about';
import { openUrl } from '@/lib/ipc';

export function About({ info, onBack }: { info: AboutInfo; onBack: () => void }) {
  return (
    <section className="about">
      <h1>Claude Usage Monitor</h1>
      <p className="version">
        Version {info.version} · {info.os}
      </p>
      <p>
        A personal project by Matteo Ronchi. Not affiliated with Anthropic. Built with Tauri, Rust
        and React; MIT licensed.
      </p>
      <ul>
        {aboutLinks(info.version).map((l) => (
          <li key={l.label}>
            <button type="button" onClick={() => openUrl(l.url)}>
              {l.label}
            </button>
          </li>
        ))}
      </ul>
      <p className="thanks">
        Thanks to usage-monitor-for-claude and claude-monitor-browser-extension for the API
        research.
      </p>
      <nav className="links">
        <button type="button" onClick={onBack}>
          Back
        </button>
      </nav>
    </section>
  );
}
