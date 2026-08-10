#!/usr/bin/env node

/**
 * Sync version across all config files
 * Usage:
 *   node scripts/sync-version.mjs          # Sync current package.json version
 *   node scripts/sync-version.mjs patch    # Bump patch version + sync
 *   node scripts/sync-version.mjs minor    # Bump minor version + sync
 *   node scripts/sync-version.mjs major    # Bump major version + sync
 */

import { readFileSync, writeFileSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const rootDir = resolve(__dirname, '..');

const PKG_PATH = resolve(rootDir, 'package.json');
const CARGO_PATH = resolve(rootDir, 'src-tauri/Cargo.toml');
const TAURI_CONF_PATH = resolve(rootDir, 'src-tauri/tauri.conf.json');

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf-8'));
}

function writeJson(path, data) {
  writeFileSync(path, JSON.stringify(data, null, 2) + '\n', 'utf-8');
}

function bumpVersion(version, type) {
  const [major, minor, patch] = version.split('.').map(Number);
  switch (type) {
    case 'patch': return `${major}.${minor}.${patch + 1}`;
    case 'minor': return `${major}.${minor + 1}.0`;
    case 'major': return `${major + 1}.0.0`;
    default: return version;
  }
}

function updateCargoToml(version) {
  let content = readFileSync(CARGO_PATH, 'utf-8');
  content = content.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`);
  writeFileSync(CARGO_PATH, content, 'utf-8');
}

function updateTauriConf(version) {
  const conf = readJson(TAURI_CONF_PATH);
  conf.version = version;
  writeJson(TAURI_CONF_PATH, conf);
}

// Main
const bumpType = process.argv[2]; // patch | minor | major | undefined

const pkg = readJson(PKG_PATH);
const oldVersion = pkg.version;

let newVersion;
if (bumpType && ['patch', 'minor', 'major'].includes(bumpType)) {
  newVersion = bumpVersion(oldVersion, bumpType);
  pkg.version = newVersion;
  writeJson(PKG_PATH, pkg);
  console.log(`Version bumped: ${oldVersion} → ${newVersion}`);
} else {
  newVersion = oldVersion;
  console.log(`Syncing version: ${newVersion}`);
}

updateCargoToml(newVersion);
console.log(`  ✓ src-tauri/Cargo.toml`);

updateTauriConf(newVersion);
console.log(`  ✓ src-tauri/tauri.conf.json`);

console.log(`\nDone! Remember to commit and tag:`);
console.log(`  git add .`);
console.log(`  git commit -m "chore: bump version to ${newVersion}"`);
console.log(`  git tag v${newVersion}`);
console.log(`  git push origin master && git push origin v${newVersion}`);
