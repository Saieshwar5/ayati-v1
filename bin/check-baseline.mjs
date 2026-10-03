#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const readJson = (relative) => JSON.parse(readFileSync(path.join(root, relative), 'utf8'));
const pin = readJson('openclaw-baseline.json');
const source = readJson(`${pin.directory}/package.json`);
const product = readJson('package.json');
const policy = readJson('config/openclaw.json');
const failures = [];
const check = (ok, message) => { if (!ok) failures.push(message); };
check(source.version === pin.version, 'Upstream package version differs from the recorded baseline.');
check(product.packageManager === source.packageManager, 'Product and upstream package managers differ.');
check(source.packageManager.startsWith(`pnpm@${pin.pnpm}+`), 'Pinned pnpm version differs.');
check(process.versions.node === pin.node, `Use Node ${pin.node}; found ${process.versions.node}.`);
check(product.engines.node === pin.node, 'Root Node engine differs from the recorded toolchain.');
check(readFileSync(path.join(root, '.node-version'), 'utf8').trim() === pin.node,
  '.node-version differs from the recorded toolchain.');
const lock = readFileSync(path.join(root, pin.directory, 'pnpm-lock.yaml'));
check(createHash('sha256').update(lock).digest('hex') === pin.lockfileSha256,
  'Dependency lock differs from the recorded baseline; review and update the pin deliberately.');
check(policy.update.checkOnStart === false && policy.update.auto.enabled === false,
  'Gateway automatic updates must be disabled.');
check(policy.nodeHost.autoUpdate.enabled === false, 'Node automatic updates must be disabled.');
check(policy.tools.deny.includes('gateway'), 'Agent gateway administration must be disabled.');
const git = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
const baselineCommit = git('log', '--format=%H', '--fixed-strings',
  `--grep=git-subtree-split: ${pin.commit}`, '-1');
check(Boolean(baselineCommit), 'The recorded upstream commit has no subtree import in this history.');
if (baselineCommit) check(git('rev-parse', `${baselineCommit}^{tree}`) === pin.tree,
  'Subtree baseline tree differs from the recorded upstream tree.');
if (failures.length) {
  for (const message of failures) console.error(message);
  process.exit(1);
}
console.log(`Pinned OpenClaw ${pin.version} (${pin.commit.slice(0, 12)}); lock and update policy verified.`);
