import { describe, expect, it } from 'vitest';
import { aboutLinks, REPO_URL } from '@/lib/about';

describe('aboutLinks', () => {
  it('points every link at the repository and pins the release to the version', () => {
    const links = aboutLinks('0.8.2');
    expect(links.map((l) => l.label)).toEqual([
      'GitHub',
      'Release notes',
      'Report an issue',
      'License',
    ]);
    for (const l of links) expect(l.url.startsWith(REPO_URL)).toBe(true);
    expect(links[1]?.url).toBe(`${REPO_URL}/releases/tag/v0.8.2`);
  });
});
