#!/usr/bin/env node
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { pruneRetiredPluginInstallLinks } from '../lib/plugin-install-links.mjs';

const root = new URL('../', import.meta.url);
const sourceRoot = fileURLToPath(new URL('vendor/openclaw/', root));
const install = spawnSync('pnpm', ['--dir', sourceRoot, 'install', '--frozen-lockfile'], {
  cwd: fileURLToPath(root), stdio: 'inherit',
});
if (install.error) throw install.error;
if (install.status !== 0) {
  process.exit(install.status ?? 1);
}
const policy = JSON.parse(readFileSync(new URL('config/source-scope.json', root), 'utf8'));
const removed = pruneRetiredPluginInstallLinks(sourceRoot, policy);
console.log(`Runtime dependencies installed; ${removed.length} retired plugin link folders removed.`);
