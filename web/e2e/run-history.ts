import { readFile, readdir, rm } from 'node:fs/promises';
import { join } from 'node:path';

export async function retainCompletedRuns(runs: string, keep: number): Promise<void> {
  if (!Number.isInteger(keep) || keep < 1) throw new Error('Retain at least one completed run.');
  const completed: { path: string; finished: string }[] = [];
  for (const entry of await readdir(runs, { withFileTypes: true })) {
    if (!entry.isDirectory() || !entry.name.startsWith('run-')) continue;
    const path = join(runs, entry.name);
    try {
      const saved = JSON.parse(await readFile(join(path, 'manifest.json'), 'utf8')) as {
        kind?: string; id?: string; finished_at?: string;
      };
      if (saved.kind === 'ayati-browser-test' && saved.id === entry.name &&
          typeof saved.finished_at === 'string' && Number.isFinite(Date.parse(saved.finished_at))) {
        completed.push({ path, finished: saved.finished_at });
      }
    } catch { /* Incomplete or foreign directories are never removed. */ }
  }
  completed.sort((a, b) => b.finished.localeCompare(a.finished));
  for (const old of completed.slice(keep)) await rm(old.path, { recursive: true });
}
