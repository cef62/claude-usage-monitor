// Mirrors package.json's version into src-tauri/tauri.conf.json.
// Changesets bumps package.json; Tauri reads tauri.conf.json. Cargo.toml stays 0.0.0 on purpose.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function syncVersion(pkgPath, confPath) {
  const { version } = JSON.parse(readFileSync(pkgPath, 'utf8'));
  if (typeof version !== 'string' || version.length === 0) {
    throw new Error(`no version in ${pkgPath}`);
  }
  const raw = readFileSync(confPath, 'utf8');
  // A JSON.parse/stringify round-trip would reformat tauri.conf.json's inline arrays
  // (e.g. "capabilities": ["default"]) onto multiple lines. Replace the version value
  // in place instead, so only that one field changes.
  const pattern = /"version"\s*:\s*"[^"]*"/;
  if (!pattern.test(raw)) {
    throw new Error(`no "version" field in ${confPath}`);
  }
  const next = raw.replace(pattern, `"version": "${version}"`);
  writeFileSync(confPath, next);
  return version;
}

const invokedDirectly =
  process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const version = syncVersion(
    resolve(root, 'package.json'),
    resolve(root, 'src-tauri/tauri.conf.json'),
  );
  console.log(`tauri.conf.json version -> ${version}`);
}
