import { api } from './types';
import { element } from './view';
import type { Profile } from './agents';

export function installProfileEditor(panel: HTMLDialogElement): (profile: Profile) => void {
  let current: Profile | undefined;
  let opened: Profile | undefined;
  let busy = false;
  const launch = element('button', 'files-button', 'Edit agent settings');
  launch.disabled = true;
  panel.querySelector('.details-note')!.before(launch);
  const dialog = document.createElement('dialog'); dialog.className = 'agent-setup';
  dialog.setAttribute('aria-labelledby', 'profile-edit-title');
  dialog.innerHTML = `<form><div class="panel-header"><h2 id="profile-edit-title">Edit agent</h2><button type="button" class="icon-button" aria-label="Close profile editor">×</button></div>
    <label>Name<input name="name" required maxlength="80"></label>
    <label>Responsibilities<textarea name="role" maxlength="4000"></textarea></label>
    <label>Model<input name="model" required maxlength="200"></label>
    <p>Use a model ID supported by the configured provider. Saving does not verify availability or make a paid call. If a request fails, you can select the previous model again.</p>
    <p>Changing responsibilities or model starts fresh model context on the next request. Saved conversations, files and memory remain available, but the old conversation is not replayed to the model. A name-only change keeps context.</p>
    <p>Finish or stop active and queued work first. Future scheduled work will use the new responsibilities. Tools and provider stay unchanged.</p>
    <label><input id="model-test-consent" type="checkbox"> Send “Reply with OK.” to this model. Normal API charges may apply; no agent history or tools are sent.</label>
    <button class="files-button" id="model-test" type="button" disabled>Test model (optional)</button><p id="model-test-result" role="status"></p>
    <p id="profile-edit-result" role="status"></p><button class="files-button" type="submit">Save changes</button></form>`;
  document.body.append(dialog);
  const form = dialog.querySelector<HTMLFormElement>('form')!;
  const name = form.elements.namedItem('name') as HTMLInputElement;
  const role = form.elements.namedItem('role') as HTMLTextAreaElement;
  const model = form.elements.namedItem('model') as HTMLInputElement;
  const consent = form.querySelector<HTMLInputElement>('#model-test-consent')!;
  const test = form.querySelector<HTMLButtonElement>('#model-test')!;
  const testResult = form.querySelector('#model-test-result')!;
  const save = form.querySelector<HTMLButtonElement>('[type=submit]')!;
  const result = form.querySelector('#profile-edit-result')!;
  launch.onclick = () => {
    if (!current) return;
    opened = { ...current }; name.value = opened.name; role.value = opened.role; model.value = opened.model;
    consent.checked = false; test.disabled = true; testResult.textContent = '';
    result.textContent = ''; dialog.showModal(); name.focus();
  };
  form.querySelector('button')!.onclick = () => { if (!busy) dialog.close(); };
  dialog.addEventListener('cancel', event => { if (busy) event.preventDefault(); });
  dialog.addEventListener('close', () => launch.focus());
  form.onsubmit = async event => {
    event.preventDefault(); if (busy || !opened) return;
    busy = true; save.disabled = true; name.disabled = true; role.disabled = true; model.disabled = true; test.disabled = true; consent.disabled = true;
    result.textContent = 'Saving…'; testResult.textContent = '';
    try {
      const edit = await api<{ name: string; role: string; model: string; revision: number }>(`/agents/${encodeURIComponent(opened.id)}/profile`, {
        method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Ayati-Agent': 'local-agent' },
        body: JSON.stringify({ name: name.value, role: role.value, model: model.value, expected_revision: opened.revision }),
      });
      opened = { ...opened, ...edit }; current = opened;
      result.textContent = 'Saved. The agent list and details will refresh shortly.';
    } catch (error) {
      result.textContent = `${error instanceof Error ? error.message : 'Save result unavailable.'} If the response was lost, reopen this editor after the details refresh before trying again.`;
    } finally { busy = false; save.disabled = false; name.disabled = false; role.disabled = false; model.disabled = false; consent.disabled = false; consent.checked = false; }
  };
  function clearTest(): void { consent.checked = false; test.disabled = true; testResult.textContent = ''; }
  model.oninput = clearTest;
  consent.onchange = () => { test.disabled = busy || !consent.checked || !model.validity.valid; };
  test.onclick = async () => {
    if (busy || !consent.checked || !model.validity.valid) return;
    busy = true; save.disabled = true; test.disabled = true; model.disabled = true; consent.disabled = true;
    const candidate = model.value;
    testResult.textContent = `Testing ${candidate} once…`;
    try {
      const response = await api<{ message: string }>('/setup/test', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ confirm: true, model: candidate }) });
      testResult.textContent = `${candidate}: ${response.message} Settings have not been saved.`;
    } catch (error) { testResult.textContent = `${candidate}: ${error instanceof Error ? error.message : 'Result unavailable.'} No automatic retry was sent. Settings have not been saved.`; }
    finally { busy = false; save.disabled = false; model.disabled = false; consent.disabled = false; consent.checked = false; }
  };
  return profile => { current = profile; launch.disabled = !profile.editable; };
}
