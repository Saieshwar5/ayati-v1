import { randomBytes } from 'node:crypto';
import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import path from 'node:path';

export function applyManagedUpdatePolicy(config) {
  return {
    ...config,
    update: { ...config.update, checkOnStart: false, auto: { enabled: false } },
    nodeHost: { ...config.nodeHost, autoUpdate: { enabled: false } },
    tools: { ...config.tools, deny: [...new Set([...(config.tools?.deny ?? []), 'gateway'])] },
    commands: { ...config.commands, config: false, restart: false },
  };
}

export function prepareRuntimeConfig(root, stateDirectory) {
  mkdirSync(stateDirectory, { recursive: true, mode: 0o700 });
  const configPath = path.join(stateDirectory, 'openclaw.json');
  let config;
  try {
    config = JSON.parse(readFileSync(configPath, 'utf8'));
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    config = JSON.parse(readFileSync(path.join(root, 'config/openclaw.json'), 'utf8'));
    config.gateway.auth.token = randomBytes(32).toString('hex');
    config.agents = { defaults: { workspace: path.join(stateDirectory, 'workspace') } };
  }
  const managedConfig = applyManagedUpdatePolicy(config);
  const temporaryPath = `${configPath}.${process.pid}.tmp`;
  writeFileSync(temporaryPath, `${JSON.stringify(managedConfig, null, 2)}\n`, { mode: 0o600 });
  renameSync(temporaryPath, configPath);
  return configPath;
}

export function runtimeEnvironment(stateDirectory, configPath, inherited = process.env) {
  return {
    ...inherited,
    OPENCLAW_STATE_DIR: stateDirectory,
    OPENCLAW_CONFIG_PATH: configPath,
    OPENCLAW_NO_AUTO_UPDATE: '1',
  };
}
