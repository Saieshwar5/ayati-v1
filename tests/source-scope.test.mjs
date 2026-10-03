import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { checkSourceScope } from '../lib/source-scope.mjs';

const policy = {
  allowedAppDirectories: ['ios', 'shared'],
  requiredFiles: ['apps/ios/project.yml', 'ui/public/licenses/NOTICE.md'],
  forbiddenPaths: ['apps/macos', 'scripts/package-mac-app.sh',
    'extensions/imessage', 'dist/extensions/imessage', 'dist-runtime/extensions/imessage'],
  forbiddenPluginPackages: ['@openclaw/imessage'],
  forbiddenCommands: ['mac:package'],
};

function fixture(t, fixturePolicy = policy) {
  const root = mkdtempSync(path.join(os.tmpdir(), 'ayati-source-scope-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const write = (relative, content = '') => {
    const file = path.join(root, relative);
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, content);
  };
  for (const file of fixturePolicy.requiredFiles) write(file);
  for (const name of fixturePolicy.allowedAppDirectories) {
    mkdirSync(path.join(root, 'apps', name), { recursive: true });
  }
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
  assert(failures.some((failure) => failure.includes('Retired source path')));
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

test('rejects retired plugin source, stale output and fallback install entries', (t) => {
  const { root, write } = fixture(t);
  write('extensions/imessage/index.ts');
  write('dist/extensions/imessage/index.js');
  write('dist-runtime/extensions/imessage/index.js');
  write('scripts/lib/official-external-channel-catalog.json', JSON.stringify({ entries: [
    { name: '@openclaw/imessage' }, { name: '@openclaw/telegram' },
  ] }));
  const failures = checkSourceScope(root, policy);
  assert.equal(failures.length, 4);
  assert(failures.some((failure) => failure.includes('Retired plugin advertised')));
});

test('Ayati policy rejects restored Mac-only bundled skills and permits retained skills', (t) => {
  const productPolicy = JSON.parse(readFileSync(
    new URL('../config/source-scope.json', import.meta.url), 'utf8',
  ));
  const { root, write } = fixture(t, productPolicy);
  write('skills/tmux/SKILL.md');
  assert.deepEqual(checkSourceScope(root, productPolicy), []);

  const retired = ['apple-notes', 'apple-reminders', 'bear-notes', 'things-mac', 'peekaboo'];
  for (const name of retired) write(`skills/${name}/SKILL.md`);
  assert.deepEqual(checkSourceScope(root, productPolicy).sort(), retired.map(
    (name) => `Retired source path returned: skills/${name}`,
  ).sort());
});
