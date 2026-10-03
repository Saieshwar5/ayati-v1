import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { prepareRuntimeConfig, runtimeEnvironment } from '../lib/runtime-config.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));

test('first setup creates private credentials and repeat setup preserves them', () => {
  const directory = mkdtempSync(path.join(os.tmpdir(), 'ayati-config-'));
  try {
    const configPath = prepareRuntimeConfig(root, directory);
    const original = JSON.parse(readFileSync(configPath, 'utf8'));
    assert.match(original.gateway.auth.token, /^[a-f0-9]{64}$/);
    assert.equal(statSync(configPath).mode & 0o777, 0o600);
    assert.equal(original.agents.defaults.workspace, path.join(directory, 'workspace'));
    prepareRuntimeConfig(root, directory);
    assert.equal(JSON.parse(readFileSync(configPath, 'utf8')).gateway.auth.token,
      original.gateway.auth.token);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('startup restores manual updates while preserving provider and tool settings', () => {
  const directory = mkdtempSync(path.join(os.tmpdir(), 'ayati-policy-'));
  try {
    const configPath = path.join(directory, 'openclaw.json');
    writeFileSync(configPath, JSON.stringify({
      update: { channel: 'beta', checkOnStart: true, auto: { enabled: true } },
      nodeHost: { autoUpdate: { enabled: true } },
      agents: { defaults: { model: { primary: 'custom/test' } } },
      tools: { deny: ['browser'] },
    }));
    prepareRuntimeConfig(root, directory);
    const config = JSON.parse(readFileSync(configPath, 'utf8'));
    assert.equal(config.update.checkOnStart, false);
    assert.equal(config.update.auto.enabled, false);
    assert.equal(config.nodeHost.autoUpdate.enabled, false);
    assert.equal(config.update.channel, 'beta');
    assert.equal(config.agents.defaults.model.primary, 'custom/test');
    assert.deepEqual(config.tools.deny, ['browser', 'gateway']);
    const env = runtimeEnvironment(directory, configPath, {
      OPENCLAW_STATE_DIR: '/personal/state', OPENCLAW_NO_AUTO_UPDATE: '0',
      FIREWORKS_API_KEY: 'synthetic-test-key',
    });
    assert.equal(env.OPENCLAW_STATE_DIR, directory);
    assert.equal(env.OPENCLAW_CONFIG_PATH, configPath);
    assert.equal(env.OPENCLAW_NO_AUTO_UPDATE, '1');
    assert.equal(env.FIREWORKS_API_KEY, 'synthetic-test-key');
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('invalid configuration fails without replacing existing data', () => {
  const directory = mkdtempSync(path.join(os.tmpdir(), 'ayati-invalid-config-'));
  try {
    const configPath = path.join(directory, 'openclaw.json');
    const invalid = '{broken: configuration';
    writeFileSync(configPath, invalid);
    assert.throws(() => prepareRuntimeConfig(root, directory), SyntaxError);
    assert.equal(readFileSync(configPath, 'utf8'), invalid);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
