#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { checkSourceScope } from '../lib/source-scope.mjs';

const root = new URL('../', import.meta.url);
const policy = JSON.parse(readFileSync(new URL('config/source-scope.json', root), 'utf8'));
const failures = checkSourceScope(fileURLToPath(new URL('vendor/openclaw/', root)), policy);
if (failures.length) {
  for (const message of failures) console.error(message);
  process.exit(1);
}
console.log('Desktop exclusion and retained mobile/web inputs verified.');
