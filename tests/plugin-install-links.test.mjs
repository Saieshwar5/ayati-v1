import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { pruneRetiredPluginInstallLinks } from '../lib/plugin-install-links.mjs';

const policy = { forbiddenPaths: ['apps/macos', 'extensions/apple-fm', 'extensions/imessage'] };
function fixture(t) {
  const root = mkdtempSync(path.join(os.tmpdir(), 'ayati-plugin-links-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const plugin = (id) => path.join(root, 'extensions', id);
  for (const id of ['apple-fm', 'imessage', 'telegram']) {
    mkdirSync(path.join(plugin(id), 'node_modules'), { recursive: true });
  }
  return { root, plugin };
}

test('removes only retired link folders and preserves dependency targets and retained plugins', (t) => {
  const { root, plugin } = fixture(t);
  const dependency = path.join(root, 'shared-dependency');
  mkdirSync(dependency);
  writeFileSync(path.join(dependency, 'index.js'), 'shared');
  symlinkSync(dependency, path.join(plugin('imessage'), 'node_modules', 'shared'));
  assert.deepEqual(pruneRetiredPluginInstallLinks(root, policy), [
    'extensions/apple-fm', 'extensions/imessage',
  ]);
  assert(!existsSync(plugin('imessage')));
  assert(existsSync(path.join(dependency, 'index.js')));
  assert(existsSync(plugin('telegram')));
  assert.deepEqual(pruneRetiredPluginInstallLinks(root, policy), []);
});

test('refuses all cleanup if any retired plugin contains restored source', (t) => {
  const { root, plugin } = fixture(t);
  writeFileSync(path.join(plugin('imessage'), 'index.ts'), 'restored source');
  assert.throws(() => pruneRetiredPluginInstallLinks(root, policy), /source returned/);
  assert(existsSync(path.join(plugin('imessage'), 'index.ts')));
  assert(existsSync(path.join(plugin('apple-fm'), 'node_modules')));
});

test('refuses substituted plugin directories and dependency links', (t) => {
  const { root, plugin } = fixture(t);
  rmSync(plugin('imessage'), { recursive: true });
  symlinkSync(plugin('telegram'), plugin('imessage'));
  assert.throws(() => pruneRetiredPluginInstallLinks(root, policy), /replaced retired path/);
  rmSync(plugin('imessage'));
  mkdirSync(plugin('imessage'));
  symlinkSync(path.join(plugin('telegram'), 'node_modules'),
    path.join(plugin('imessage'), 'node_modules'));
  assert.throws(() => pruneRetiredPluginInstallLinks(root, policy), /replaced dependency directory/);
});
