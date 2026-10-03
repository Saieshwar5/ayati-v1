#!/usr/bin/env node
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { prepareRuntimeConfig, runtimeEnvironment } from '../lib/runtime-config.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const environmentFile = path.join(root, '.env');
if (existsSync(environmentFile)) process.loadEnvFile(environmentFile);
const stateDirectory = path.resolve(root, process.env.AYATI_STATE_DIR || '.ayati-openclaw');
const [command, ...args] = process.argv.slice(2);
if (!['init', 'start', 'openclaw'].includes(command)) {
  console.error('Usage: node bin/ayati.mjs init|start|openclaw [OpenClaw arguments]');
  process.exit(2);
}
if (command === 'openclaw' && args.includes('update')) {
  console.error('Ayati upgrades are managed in Git. Follow docs/upstream-upgrades.md.');
  process.exit(2);
}
let configPath;
try {
  configPath = prepareRuntimeConfig(root, stateDirectory);
} catch (error) {
  console.error(error instanceof SyntaxError
    ? 'Invalid JSON in the private OpenClaw configuration. Existing data was not replaced.'
    : `Could not prepare private OpenClaw configuration (${error.code ?? 'configuration error'}).`);
  process.exit(1);
}
if (command === 'init') {
  console.log(`Private OpenClaw configuration ready at ${configPath}.`);
} else {
  const runtime = path.join(root, 'vendor/openclaw');
  if (!existsSync(path.join(runtime, 'dist/entry.js')) &&
      !existsSync(path.join(runtime, 'dist/entry.mjs'))) {
    console.error('OpenClaw has not been built. Run pnpm setup:runtime and pnpm build:runtime.');
    process.exit(1);
  }
  const cliArgs = command === 'start' ? ['gateway', 'run', ...args] : args;
  const child = spawn(process.execPath, ['openclaw.mjs', ...cliArgs], {
    cwd: runtime,
    env: runtimeEnvironment(stateDirectory, configPath),
    stdio: 'inherit',
  });
  for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill(signal));
  child.on('error', (error) => {
    console.error(`Could not launch OpenClaw: ${error.message}`);
    process.exitCode = 1;
  });
  child.on('exit', (code, signal) => {
    process.exitCode = code ?? (signal === 'SIGINT' ? 130 : 143);
  });
}
