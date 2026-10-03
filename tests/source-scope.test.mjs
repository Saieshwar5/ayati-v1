import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { checkSourceScope } from '../lib/source-scope.mjs';

const policy = {
  allowedAppDirectories: ['ios', 'shared'],
  requiredFiles: ['apps/ios/project.yml', 'ui/public/licenses/NOTICE.md'],
  forbiddenPaths: ['apps/macos', 'scripts/package-mac-app.sh'],
  forbiddenCommands: ['mac:package'],
};

function fixture(t) {
  const root = mkdtempSync(path.join(os.tmpdir(), 'ayati-source-scope-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (relative, content = '') => {
    const file = path.join(root, relative);
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, content);
  };
  for (const file of policy.requiredFiles) write(file);
  mkdirSync(path.join(root, 'apps/shared'));
  write('package.json', JSON.stringify({ scripts: { build: 'node scripts/build.mjs' } }));
  return { root, write };
}

test('accepts retained sources and ordinary runtime or browser-viewer output', (t) => {
  const { root, write } = fixture(t);
  write('dist/control-ui/cloud-desktop-viewer.js');
  write('dist/mac-node-worker.js');
  assert.deepEqual(checkSourceScope(root, policy), []);
});

test('rejects a new application directory and resurrected untracked tooling', (t) => {
  const { root, write } = fixture(t);
  write('apps/windows/main.rs');
  write('scripts/package-mac-app.sh');
  const failures = checkSourceScope(root, policy);
  assert(failures.some((failure) => failure.includes('apps/windows')));
  assert(failures.some((failure) => failure.includes('package-mac-app.sh')));
});

test('rejects missing retained input and symbolic links replacing app ownership', (t) => {
  const { root } = fixture(t);
  rmSync(path.join(root, 'ui/public/licenses/NOTICE.md'));
  symlinkSync('ios', path.join(root, 'apps/macos'));
  const failures = checkSourceScope(root, policy);
  assert(failures.some((failure) => failure.includes('Required retained file')));
  assert(failures.some((failure) => failure.includes('Retired desktop path')));
});

test('rejects retired commands and indirect commands pointing at removed scripts', (t) => {
  const { root, write } = fixture(t);
  write('package.json', JSON.stringify({ scripts: {
    'mac:package': 'desktop-build', other: 'bash scripts/package-mac-app.sh',
  } }));
  assert.equal(checkSourceScope(root, policy).length, 2);
});

test('rejects nested desktop artifacts after building', (t) => {
  const { root, write } = fixture(t);
  write('dist/installers/OpenClaw.AppImage');
  mkdirSync(path.join(root, 'dist/installers/OpenClaw.app'));
  assert.equal(checkSourceScope(root, policy).length, 2);
});
