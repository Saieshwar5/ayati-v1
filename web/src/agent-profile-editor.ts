import { api } from './types';
import { element } from './view';
import type { Profile } from './agents';

export function installProfileEditor(panel: HTMLDialogElement): (profile: Profile) => void {
  let current: Profile | undefined;
  let opened: Profile | undefined;
  let busy = false;
  const launch = element('button', 'files-button', 'Edit name and responsibilities');
  launch.disabled = true;
  panel.querySelector('.details-note')!.before(launch);
  const dialog = document.createElement('dialog'); dialog.className = 'agent-setup';
  dialog.setAttribute('aria-labelledby', 'profile-edit-title');
  dialog.innerHTML = `<form><div class="panel-header"><h2 id="profile-edit-title">Edit agent</h2><button type="button" class="icon-button" aria-label="Close profile editor">×</button></div>
    <label>Name<input name="name" required maxlength="80"></label>
    <label>Responsibilities<textarea name="role" maxlength="4000"></textarea></label>
    <p>Changing responsibilities starts fresh model context on the next request. Saved conversations, files and memory remain available, but the old conversation is not replayed to the model. A name-only change keeps context.</p>
    <p>Finish or stop active and queued work first. Future scheduled work will use the new responsibilities. Tools and model stay unchanged.</p>
    <p id="profile-edit-result" role="status"></p><button class="files-button" type="submit">Save changes</button></form>`;
  document.body.append(dialog);
  const form = dialog.querySelector<HTMLFormElement>('form')!;
  const name = form.elements.namedItem('name') as HTMLInputElement;
  const role = form.elements.namedItem('role') as HTMLTextAreaElement;
  const save = form.querySelector<HTMLButtonElement>('[type=submit]')!;
  const result = form.querySelector('#profile-edit-result')!;
  launch.onclick = () => {
    if (!current) return;
    opened = { ...current }; name.value = opened.name; role.value = opened.role;
    result.textContent = ''; dialog.showModal(); name.focus();
  };
  form.querySelector('button')!.onclick = () => { if (!busy) dialog.close(); };
  dialog.addEventListener('cancel', event => { if (busy) event.preventDefault(); });
  dialog.addEventListener('close', () => launch.focus());
  form.onsubmit = async event => {
    event.preventDefault(); if (busy || !opened) return;
    busy = true; save.disabled = true; name.disabled = true; role.disabled = true;
    result.textContent = 'Saving…';
    try {
      const edit = await api<{ name: string; role: string; revision: number }>(`/agents/${encodeURIComponent(opened.id)}/profile`, {
        method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Ayati-Agent': 'local-agent' },
        body: JSON.stringify({ name: name.value, role: role.value, expected_revision: opened.revision }),
      });
      opened = { ...opened, ...edit }; current = opened;
      result.textContent = 'Saved. The agent list and details will refresh shortly.';
    } catch (error) {
      result.textContent = `${error instanceof Error ? error.message : 'Save result unavailable.'} If the response was lost, reopen this editor after the details refresh before trying again.`;
    } finally { busy = false; save.disabled = false; name.disabled = false; role.disabled = false; }
  };
  return profile => { current = profile; launch.disabled = !profile.editable; };
}
