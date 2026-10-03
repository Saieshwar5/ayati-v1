import { lstatSync, readdirSync, rmSync, rmdirSync } from 'node:fs';
import path from 'node:path';

// Frozen pnpm installs can materialize obsolete lockfile importers as link-only
// workspaces. Preserve the upstream lock, but never delete restored source.
export function pruneRetiredPluginInstallLinks(sourceRoot, policy) {
  const candidates = [];
  for (const relative of policy.forbiddenPaths) {
    if (!/^extensions\/[a-z][a-z0-9-]*$/.test(relative)) continue;
    const directory = path.join(sourceRoot, relative);
    let stat;
    try {
      stat = lstatSync(directory);
    } catch (error) {
      if (error.code === 'ENOENT') continue;
      throw error;
    }
    if (!stat.isDirectory() || stat.isSymbolicLink()) {
      throw new Error(`Refusing install cleanup of replaced retired path: ${relative}`);
    }
    const entries = readdirSync(directory);
    if (entries.some((name) => name !== 'node_modules')) {
      throw new Error(`Retired plugin source returned; refusing install cleanup: ${relative}`);
    }
    const links = path.join(directory, 'node_modules');
    if (entries.length && !lstatSync(links).isDirectory()) {
      throw new Error(`Refusing install cleanup of replaced dependency directory: ${relative}`);
    }
    candidates.push({ relative, directory, links });
  }
  // Validate every owner before any removal. rmdir also preserves a source file
  // created after validation instead of recursively deleting the plugin root.
  for (const { directory, links } of candidates) {
    rmSync(links, { recursive: true, force: true });
    rmdirSync(directory);
  }
  return candidates.map(({ relative }) => relative);
}
