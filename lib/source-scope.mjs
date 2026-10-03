import { lstatSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

function statIfPresent(file) {
  try {
    return lstatSync(file);
  } catch (error) {
    if (error.code === 'ENOENT') return undefined;
    throw error;
  }
}

// Check the working tree too: a generator can resurrect untracked desktop code.
export function checkSourceScope(sourceRoot, policy) {
  const failures = [];
  const allowed = new Set(policy.allowedAppDirectories);
  const apps = path.join(sourceRoot, 'apps');
  for (const entry of readdirSync(apps, { withFileTypes: true })) {
    if (!allowed.has(entry.name) || !entry.isDirectory()) {
      failures.push(`Unreviewed application path: apps/${entry.name}`);
    }
  }
  for (const relative of policy.forbiddenPaths) {
    if (statIfPresent(path.join(sourceRoot, relative))) {
      failures.push(`Retired desktop path returned: ${relative}`);
    }
  }
  for (const relative of policy.requiredFiles) {
    const stat = statIfPresent(path.join(sourceRoot, relative));
    if (!stat?.isFile()) failures.push(`Required retained file missing or replaced: ${relative}`);
  }
  const scripts = JSON.parse(readFileSync(path.join(sourceRoot, 'package.json'), 'utf8')).scripts;
  for (const name of policy.forbiddenCommands) {
    if (Object.hasOwn(scripts, name)) failures.push(`Retired desktop command returned: ${name}`);
  }
  for (const [name, command] of Object.entries(scripts)) {
    if (policy.forbiddenPaths.some((relative) => command.includes(relative))) {
      failures.push(`Command ${name} references retired desktop source.`);
    }
  }
  // Native GUI installers are never outputs of Ayati's server/web build.
  // Generic OS CLI binaries and cloud browser viewers are legitimate runtime features.
  const dist = path.join(sourceRoot, 'dist');
  if (statIfPresent(dist)) {
    const visit = (directory) => {
      for (const entry of readdirSync(directory, { withFileTypes: true })) {
        const file = path.join(directory, entry.name);
        if (/\.(?:app|appimage|dmg|msi)$/i.test(entry.name)
          || /^openclaw-desktop.*\.exe$/i.test(entry.name)) {
          failures.push(`Desktop installer in build output: ${path.relative(sourceRoot, file)}`);
        } else if (entry.isDirectory() && entry.name !== 'node_modules') {
          visit(file);
        }
      }
    };
    // Frozen plugin dependencies have pnpm links. Inspect our emitted artifacts,
    // not their dependency trees; this is a regression guard, not a package audit.
    visit(dist);
  }
  return failures;
}
