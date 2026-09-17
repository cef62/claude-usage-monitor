import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { syncVersion } from '../scripts/sync-version.mjs';

function fixture(pkgVersion: string, confVersion: string) {
  const dir = mkdtempSync(join(tmpdir(), 'sync-version-'));
  const pkg = join(dir, 'package.json');
  const conf = join(dir, 'tauri.conf.json');
  writeFileSync(pkg, `${JSON.stringify({ name: 'x', version: pkgVersion }, null, 2)}\n`);
  writeFileSync(
    conf,
    `${JSON.stringify({ productName: 'X', version: confVersion, build: {} }, null, 2)}\n`,
  );
  return { pkg, conf };
}

describe('syncVersion', () => {
  it('copies package.json version into tauri.conf.json', () => {
    const { pkg, conf } = fixture('1.2.3', '0.0.0');
    expect(syncVersion(pkg, conf)).toBe('1.2.3');
    expect(JSON.parse(readFileSync(conf, 'utf8')).version).toBe('1.2.3');
  });

  it('preserves formatting and trailing newline', () => {
    const { pkg, conf } = fixture('1.2.3', '1.2.3');
    const before = readFileSync(conf, 'utf8');
    syncVersion(pkg, conf);
    expect(readFileSync(conf, 'utf8')).toBe(before);
  });

  it('throws when package.json has no version', () => {
    const { pkg, conf } = fixture('1.0.0', '1.0.0');
    writeFileSync(pkg, '{"name":"x"}\n');
    expect(() => syncVersion(pkg, conf)).toThrow(/version/);
  });
});
