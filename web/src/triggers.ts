import { api, post, selectedAgent } from './types';
import { element } from './view';
import { installEventSourceForm } from './event-sources';

type Trigger = { id: string; connection_id: string; name: string; slug: string; provider_id: string; instructions: string; state: string; revision: number };
type Source = { id: string; slug: string };

export function installTriggers(container: HTMLElement): () => Promise<void> {
  container.classList.add('connections');
  container.innerHTML = `<h3>Event triggers</h3><p>Start this agent’s work when a connected account sends a matching event. Model charges and tool approvals still apply. This computer and Relay must stay on.</p>
    <button class="files-button" data-trigger-refresh>Refresh triggers</button><p data-trigger-status role="status"></p>
    <div data-trigger-list></div><div data-trigger-runs></div>
    <form data-trigger-form><h3>Create a trigger</h3>
      <label>Account<select name="connection" required></select></label>
      <button type="button" class="files-button" data-source-refresh>Load event subscriptions</button>
      <button type="button" class="files-button" data-source-create>Watch a new event</button>
      <label>Event subscription<select name="source" required></select></label>
      <label>Trigger name<input name="name" required maxlength="100"></label>
      <label>Task instructions<textarea name="instructions" required maxlength="8000"></textarea></label>
      <details><summary>Optional event filter</summary><p>Match an exact text value in one event field. Leave both empty to accept every event from the chosen subscription.</p><label>Event field<input name="field" maxlength="128"></label><label>Equals<input name="value" maxlength="500"></label></details>
      <button class="files-button" type="submit">Create trigger</button>
      <button class="files-button" type="button" data-trigger-new hidden>Start a new draft</button>
    </form><div data-source-editor></div>`;
  const form = container.querySelector<HTMLFormElement>('[data-trigger-form]')!;
  const field = <T extends HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>(name: string): T => form.elements.namedItem(name) as T;
  const connection = field<HTMLSelectElement>('connection');
  const source = field<HTMLSelectElement>('source');
  const list = container.querySelector<HTMLElement>('[data-trigger-list]')!;
  const runs = container.querySelector<HTMLElement>('[data-trigger-runs]')!;
  const status = container.querySelector<HTMLElement>('[data-trigger-status]')!;
  const editor = container.querySelector<HTMLElement>('[data-source-editor]')!;
  let busy = false, fresh = false, fingerprint = '';
  let sources: Source[] = [];
  let pending: Record<string, unknown> | undefined;
  const pendingKey = `ayati.trigger.pending.${selectedAgent}`;
  try { const saved = sessionStorage.getItem(pendingKey); if (saved) pending = JSON.parse(saved) as Record<string, unknown>; } catch { /* Storage is optional. */ }
  function controls(): void {
    container.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = busy || !fresh; });
    for (const input of form.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>('input,select,textarea')) input.disabled = busy || !fresh || !!pending;
    form.querySelector<HTMLButtonElement>('[type=submit]')!.textContent = pending ? 'Retry same trigger save' : 'Create trigger';
    container.querySelector<HTMLElement>('[data-trigger-new]')!.hidden = !pending;
  }
  async function act(action: () => Promise<void>): Promise<void> {
    if (busy) return;
    busy = true; controls();
    try { await action(); }
    catch (error) { status.textContent = `${(error as Error).message} Refresh the saved state before retrying.`; }
    finally { busy = false; controls(); }
  }
  async function loadSources(): Promise<void> {
    source.replaceChildren(); sources = [];
    if (!connection.value) return;
    sources = (await api<{ sources: Source[] }>(`/connections/${connection.value}/event-sources`)).sources;
    for (const item of sources) { const option = document.createElement('option'); option.value = item.id; option.textContent = item.slug; source.append(option); }
    if (!sources.length) status.textContent = 'No subscriptions yet. Choose Watch a new event to register one.';
  }
  async function refresh(): Promise<void> {
    try {
      const [data, accounts] = await Promise.all([api<{ triggers: Trigger[]; webhook_configured: boolean; event_management_supported: boolean }>('/triggers'), api<{ connections: { id: string; label: string; status: string }[] }>('/connections')]);
      fresh = true;
      status.textContent = data.webhook_configured ? 'Triggers loaded. External delivery also needs a Composio webhook URL.' : 'Triggers loaded. Configure COMPOSIO_WEBHOOK_SECRET and webhook delivery before expecting external events.';
      form.hidden = !data.event_management_supported;
      if (!data.event_management_supported) status.textContent = 'Composio Connect MCP supports account tools. External event subscription management is unavailable through this MCP catalog; schedules can still run assigned operations.';
      const available = accounts.connections.filter(c => c.status === 'ACTIVE');
      if (JSON.stringify(available) !== connection.dataset.accounts) {
        const selected = connection.value; connection.replaceChildren();
        for (const account of available) { const option = document.createElement('option'); option.value = account.id; option.textContent = account.label; connection.append(option); }
        if (available.some(c => c.id === selected)) connection.value = selected;
        connection.dataset.accounts = JSON.stringify(available); source.replaceChildren(); sources = [];
      }
      const next = JSON.stringify(data.triggers);
      if (next !== fingerprint) {
        fingerprint = next; list.replaceChildren();
        if (!data.triggers.length) list.append(element('p', '', 'No event triggers for this agent.'));
        for (const trigger of data.triggers) {
          const card = element('article', 'connection-card'); card.append(element('strong', '', trigger.name), element('p', '', `${trigger.state} · ${trigger.slug}`), element('p', '', trigger.instructions));
          const edit = element('button', 'files-button', 'Edit trigger'); edit.onclick = () => {
            const editForm = document.createElement('form');
            const nameLabel = document.createElement('label'); nameLabel.textContent = 'Updated trigger name';
            const name = document.createElement('input'); name.required = true; name.maxLength = 100; name.value = trigger.name; nameLabel.append(name);
            const taskLabel = document.createElement('label'); taskLabel.textContent = 'Updated task instructions';
            const task = document.createElement('textarea'); task.required = true; task.maxLength = 8000; task.value = trigger.instructions; taskLabel.append(task);
            const save = element('button', 'files-button', 'Save trigger changes'); save.type = 'submit';
            editForm.append(nameLabel, taskLabel, element('p', '', 'Saving cancels older queued occurrences. Active work keeps running. To change the account, event or filter, pause this rule and create another.'), save);
            editor.replaceChildren(editForm);
            editForm.onsubmit = event => { event.preventDefault(); void act(async () => {
              await post(`/triggers/${trigger.id}/edit`, { name: name.value, instructions: task.value, expected_revision: trigger.revision }); editor.replaceChildren(); await refresh();
            }); };
          }; card.append(edit);
          for (const [label, state] of [[trigger.state === 'active' ? 'Pause' : 'Resume', trigger.state === 'active' ? 'paused' : 'active'], ['Remove trigger', 'removed']]) {
            const button = element('button', 'files-button', label); button.onclick = () => void act(async () => {
              await post(`/triggers/${trigger.id}/state`, { state, expected_revision: trigger.revision }); await refresh();
            }); card.append(button);
          }
          const history = element('button', 'text-button', 'View event runs'); history.onclick = () => void act(async () => {
            const data = await api<{ runs: { event_id: string; state: string; task_id: string | null; task?: { status: string } }[] }>(`/triggers/${trigger.id}/runs`);
            runs.replaceChildren(element('h3', '', `Recent events · ${trigger.name}`));
            if (!data.runs.length) runs.append(element('p', '', 'No matching events yet.'));
            for (const run of data.runs) {
              const row = element('p', '', `${run.event_id} · ${run.task?.status ?? run.state}`);
              if (run.task_id) { const open = element('button', 'text-button', 'View result'); open.onclick = () => void act(async () => {
                const result = await api<{ messages: { role: string; content: string }[] }>(`/tasks/${run.task_id}/history`);
                runs.append(...result.messages.map(m => element('p', 'schedule-message', `${m.role}: ${m.content}`)));
              }); row.append(open); } runs.append(row);
            }
          }); card.append(history); list.append(card);
        }
      }
    } catch (error) { fresh = false; status.textContent = `Could not load triggers: ${(error as Error).message}`; }
    controls();
  }
  connection.onchange = () => { sources = []; source.replaceChildren(); editor.replaceChildren(); };
  container.querySelector<HTMLButtonElement>('[data-trigger-new]')!.onclick = () => {
    pending = undefined; try { sessionStorage.removeItem(pendingKey); } catch { /* Optional storage. */ }
    status.textContent = 'New draft started. Any previously saved rule remains in the list.'; controls();
  };
  container.querySelector<HTMLButtonElement>('[data-source-refresh]')!.onclick = () => void act(loadSources);
  container.querySelector<HTMLButtonElement>('[data-source-create]')!.onclick = () => void act(async () => {
    if (!connection.value) throw new Error('Connect and refresh an account in Settings first.');
    await installEventSourceForm(editor, connection.value, loadSources);
  });
  container.querySelector<HTMLButtonElement>('[data-trigger-refresh]')!.onclick = () => void act(refresh);
  form.onsubmit = event => {
    event.preventDefault(); void act(async () => {
      if (!pending) {
        const selected = sources.find(s => s.id === source.value); if (!selected) throw new Error('Load and choose an event subscription first.');
        const key = field<HTMLInputElement>('field').value;
        if (key && !/^[A-Za-z0-9_-]{1,128}$/.test(key)) throw new Error('Use a field name containing letters, numbers, underscores or hyphens.');
        pending = { id: crypto.randomUUID(), connection_id: connection.value, name: field<HTMLInputElement>('name').value, slug: selected.slug, provider_id: selected.id, instructions: field<HTMLTextAreaElement>('instructions').value, filter: key ? { [key]: field<HTMLInputElement>('value').value } : {} };
        try { sessionStorage.setItem(pendingKey, JSON.stringify(pending)); } catch { /* Retain pending receipt in memory. */ }
      }
      await post('/triggers', pending); pending = undefined;
      try { sessionStorage.removeItem(pendingKey); } catch { /* Optional storage. */ }
      field<HTMLInputElement>('name').value = ''; field<HTMLTextAreaElement>('instructions').value = ''; await refresh();
    });
  };
  return async () => { await act(refresh); };
}
