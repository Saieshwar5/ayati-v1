import { strToU8, unzipSync, zipSync } from 'fflate';
import { identityRequest, workspaceId } from '../../../workspace-context';

/** One file of a skill, as text (D234). */
export type SkillFile = { path: string; text: string };
export type Skill = {
  id: string; name: string; description: string; shared: boolean; owner_id: string; owner_name: string;
  version: number; license: string | null; has_scripts: boolean; updated_ms: number; files: number; agents: string[];
  used_this_week?: number;
};
export type SkillDetail = Omit<Skill, 'files'> & { files: SkillFile[]; hash: string; shown_version: number;
  versions: { version: number; hash: string; author: string; created_ms: number }[] };
export type Checked = { name: string; description: string; license: string | null; has_scripts: boolean; hash: string; bytes: number };
export type Library = { workspace: Skill[]; mine: Skill[]; others: Skill[]; admin: boolean };

const base = `/workspaces/${workspaceId}`;
export const loadLibrary = () => identityRequest<Library>(`${base}/skills`);
export const loadAgentSkills = (agent: string) => identityRequest<{ skills: Skill[]; editable: boolean }>(`${base}/agent-profiles/${encodeURIComponent(agent)}/skills`);
export const assignSkill = (agent: string, skill: string, on: boolean) =>
  identityRequest<{ skills: Skill[]; editable: boolean }>(`${base}/agent-profiles/${encodeURIComponent(agent)}/skills`, { skill, on });
export const checkSkill = (files: SkillFile[]) => identityRequest<Checked>(`${base}/skills/check`, { files });
export const createSkill = (id: string, shared: boolean, files: SkillFile[]) => identityRequest<SkillDetail>(`${base}/skills`, { id, shared, files });
export const loadSkill = (id: string, version?: number) =>
  identityRequest<SkillDetail>(`${base}/skills/${encodeURIComponent(id)}${version ? `?version=${version}` : ''}`);

/** The same limits as the server, checked before anything is read in full. */
const MAX_BYTES = 5 * 1024 * 1024;
const MAX_FILES = 100;
const ignored = (path: string) => path.endsWith('/') || path.split('/').some(part => part === '__MACOSX' || part === '.DS_Store');
const decoder = () => new TextDecoder('utf-8', { fatal: true });

/** Paths inside one top folder (as zips of a folder have) start at its contents. */
function strip(files: SkillFile[]): SkillFile[] {
  const tops = new Set(files.map(file => file.path.split('/')[0]));
  const [top] = tops;
  if (tops.size !== 1 || files.some(file => !file.path.includes('/'))) return files;
  return files.map(file => ({ ...file, path: file.path.slice(top.length + 1) }));
}

function text(path: string, bytes: Uint8Array): SkillFile {
  try { return { path, text: decoder().decode(bytes) }; }
  catch { throw new Error(`${path} isn't a text file. Skills can hold text files and script sources for now.`); }
}

/** A `.zip` of a skill. Sizes are checked from the archive before unpacking, and
 * again after, so a small archive can't expand into something huge. */
export function readZip(data: Uint8Array): SkillFile[] {
  let claimed = 0, count = 0;
  const entries = unzipSync(data, { filter: file => {
    if (ignored(file.name)) return false;
    claimed += file.originalSize; count += 1;
    if (claimed > MAX_BYTES) throw new Error('A skill can be at most 5 MB.');
    if (count > MAX_FILES) throw new Error(`A skill can have at most ${MAX_FILES} files.`);
    return true;
  } });
  const files = Object.entries(entries).map(([path, bytes]) => {
    if (bytes.length > MAX_BYTES) throw new Error('A skill can be at most 5 MB.');
    return text(path, bytes);
  });
  return strip(files);
}

type Found = { path: string; file: File };

async function readFound(found: Found[]): Promise<SkillFile[]> {
  const chosen = found.filter(item => !ignored(item.path));
  if (chosen.length > MAX_FILES) throw new Error(`A skill can have at most ${MAX_FILES} files.`);
  if (chosen.reduce((sum, item) => sum + item.file.size, 0) > MAX_BYTES) throw new Error('A skill can be at most 5 MB.');
  const files = await Promise.all(chosen.map(async item => text(item.path, new Uint8Array(await item.file.arrayBuffer()))));
  return strip(files);
}

/** A folder chosen with the browser's folder picker. */
export function readFolder(list: FileList | File[]): Promise<SkillFile[]> {
  return readFound([...list].map(file => ({ path: file.webkitRelativePath || file.name, file })));
}

/** Every file under a dropped folder, stopping as soon as there are too many. */
async function walk(folder: FileSystemDirectoryEntry, found: Found[]): Promise<void> {
  const reader = folder.createReader();
  for (;;) {
    // readEntries gives a folder's entries in batches, then an empty one.
    const batch = await new Promise<FileSystemEntry[]>((resolve, reject) => reader.readEntries(resolve, reject));
    if (!batch.length) return;
    for (const entry of batch) {
      if (ignored(entry.fullPath)) continue;
      if (entry.isDirectory) await walk(entry as FileSystemDirectoryEntry, found);
      else found.push({ path: entry.fullPath.replace(/^\//, ''),
        file: await new Promise<File>((resolve, reject) => (entry as FileSystemFileEntry).file(resolve, reject)) });
      if (found.length > MAX_FILES) throw new Error(`A skill can have at most ${MAX_FILES} files.`);
    }
  }
}

/** What was dropped on the upload step: one `.zip`, one skill folder or a SKILL.md.
 * The caller collects `entries` and `files` during the drop event, while they exist. */
export async function readDropped(entries: (FileSystemEntry | null)[], files: File[]): Promise<{ source: string; files: SkillFile[] }> {
  const [entry] = entries;
  if (entries.length === 1 && entry?.isDirectory) {
    const found: Found[] = [];
    await walk(entry as FileSystemDirectoryEntry, found);
    return { source: entry.name, files: await readFound(found) };
  }
  const [file] = files;
  if (files.length === 1 && /\.zip$/i.test(file.name)) return { source: file.name, files: readZip(new Uint8Array(await file.arrayBuffer())) };
  if (files.length === 1 && file.name === 'SKILL.md') return { source: file.name, files: [text('SKILL.md', new Uint8Array(await file.arrayBuffer()))] };
  throw new Error('Drop one .zip, one skill folder or a SKILL.md.');
}

/** Script sources as the server's file check counts them (`skill_files.rs`); for display only. */
const SCRIPTS = ['py', 'sh', 'js', 'ts', 'rb', 'ps1', 'bash'];
export const isScript = (path: string) => path.startsWith('scripts/') || (path.includes('.') && SCRIPTS.includes(path.split('.').pop()!.toLowerCase()));
export const textBytes = (text: string) => new TextEncoder().encode(text).length;
export const byteSize = (bytes: number) => bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`;

/** A standard `.zip` (a folder named after the skill), for Claude, Goose and others. */
export function zipSkill(name: string, files: SkillFile[]): Uint8Array {
  return zipSync(Object.fromEntries(files.map(file => [`${name}/${file.path}`, strToU8(file.text)])), { level: 6 });
}

export function download(name: string, files: SkillFile[]) {
  const blob = new Blob([zipSkill(name, files) as BlobPart], { type: 'application/zip' });
  const link = document.createElement('a');
  link.href = URL.createObjectURL(blob); link.download = `${name}.zip`;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(link.href), 1000);
}

export const scope = (skill: Pick<Skill, 'shared'>) => skill.shared ? 'Workspace skill' : 'My skill';
