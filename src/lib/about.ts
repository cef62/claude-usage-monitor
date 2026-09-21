export const REPO_URL = 'https://github.com/cef62/claude-usage-monitor';

export type About = { version: string; os: string };

export const DEFAULT_ABOUT: About = { version: '0.0.0-dev', os: 'browser' };

export type Link = { label: string; url: string };

export function aboutLinks(version: string): Link[] {
  return [
    { label: 'GitHub', url: REPO_URL },
    { label: 'Release notes', url: `${REPO_URL}/releases/tag/v${version}` },
    { label: 'Report an issue', url: `${REPO_URL}/issues/new` },
    { label: 'License', url: `${REPO_URL}/blob/main/LICENSE` },
  ];
}
