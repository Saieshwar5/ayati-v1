import type { Artifact } from './types';
import { currentView } from './view-lifetime';
import { element } from './view';
export function installArtifacts(): (files: Artifact[]) => void {
  const view = currentView();
  const { api, apiUrl } = view;
  let fingerprint = '';
  let previewRequest = 0;
  function renderArtifacts(files: Artifact[]): void {
    const visible = files.filter(f => f.kind !== 'log');
    const next = JSON.stringify(visible); if (next === fingerprint) return; fingerprint = next;
    const list = view.querySelector<HTMLElement>('#files-list')!;
    list.replaceChildren();
    if (!visible.length) list.append(element('p', 'empty-files', 'Files you share and saved outputs appear here.'));
    for (const file of visible) {
      const versions = visible.filter(f => f.kind === 'output' && f.name === file.name);
      const origin = file.kind === 'upload' ? 'Attached' : versions.length > 1 ? `Saved version ${versions.length - versions.indexOf(file)} of ${versions.length}` : 'Created';
      const row = element('div', 'artifact-row');
      const open = element('button', 'artifact-open'); open.type = 'button';
      open.append(element('span', 'file-name', file.name), element('small', 'file-meta', `${origin} · ${Math.ceil(file.size / 1024)} KB · ${new Date(file.created_at).toLocaleDateString()}`));
      open.onclick = () => void preview(file);
      const download = element('a', 'artifact-download', '↓'); download.href = apiUrl(`/files/${encodeURIComponent(file.id)}`); download.download = file.name; download.setAttribute('aria-label', `Download ${file.name}`);
      row.append(open, download); list.append(row);
    }
  }
  async function preview(file: Artifact): Promise<void> {
    const request = ++previewRequest;
    const box = view.querySelector<HTMLElement>('#artifact-preview')!;
    box.hidden = false; box.replaceChildren(element('p', '', 'Loading preview…'));
    try {
      const result = await api<{ text: string | null; truncated: boolean }>(`/files/${encodeURIComponent(file.id)}/preview`);
      if (request !== previewRequest) return;
      const close = element('button', 'text-button', 'Close preview'); close.onclick = () => { ++previewRequest; box.hidden = true; };
      box.replaceChildren(close, element('strong', '', file.name), element('pre', '', result.text ?? 'Preview unavailable for this format. Download the original to open it.'));
      if (result.truncated) box.append(element('p', '', 'Preview shortened. Download for the complete file.'));
    } catch (error) { if (request === previewRequest) box.replaceChildren(element('p', '', (error as Error).message)); }
  }

  return renderArtifacts;
}
