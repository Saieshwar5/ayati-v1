import { currentView } from './view-lifetime';
import { workPanels } from './work-panels';

import { element } from './view';
interface Memory { id: string; key: string; content: string; quote: string; source_kind: string; source_date: string; source_id: string }
export function installMemory(): void {
  const view = currentView();
  const { api, post } = view;
  const panel = element('dialog', 'memory-panel');
  panel.hidden = true; panel.setAttribute('aria-label', 'Agent memory'); panel.id = 'memory-panel';
  panel.innerHTML = `<div class="panel-header"><h2>Agent memory</h2><button class="icon-button" id="memory-close" aria-label="Close memory">×</button></div>
    <p>Facts your agent can use in future work. Saving or forgetting stops current work; send a follow-up to continue.</p>
    <form id="memory-search"><label>Search <input name="query" maxlength="256"></label><button class="text-button">Search</button></form>
    <p id="memory-status" role="status"></p><div id="memory-list"></div>
    <form id="memory-note"><label>Fact key <input name="key" required maxlength="80" pattern="[A-Za-z0-9_.-]+"></label>
    <label>Fact <textarea name="content" required maxlength="2000"></textarea></label><button class="text-button">Save fact</button><button class="text-button" type="button" id="memory-new">New fact</button></form>
    <p>Do not save passwords or secrets. Forgetting also withdraws facts from the same source and blocks automatic relearning. Original history remains.</p>`;
  view.querySelector('#app')!.append(panel);
  const button = element('button', 'files-button', 'Memory');
  button.hidden = true; button.id = 'memory-button'; button.setAttribute('aria-controls', panel.id); button.setAttribute('aria-expanded', 'false');
  view.querySelector('.chat-tool-links')!.append(button);
  const status = panel.querySelector<HTMLElement>('#memory-status')!;
  const form = panel.querySelector<HTMLFormElement>('#memory-note')!;
  const key = form.elements.namedItem('key') as HTMLInputElement;
  const content = form.elements.namedItem('content') as HTMLTextAreaElement;
  const query = panel.querySelector<HTMLInputElement>('[name="query"]')!;
  let replaces: string | undefined;
  function clear(): void { replaces = undefined; form.reset(); key.readOnly = false; }
  async function load(): Promise<void> {
    const data = await api<{ enabled: boolean; memories: Memory[] }>(`/memory?query=${encodeURIComponent(query.value)}`);
    button.hidden = !data.enabled;
    const list = panel.querySelector('#memory-list')!;
    list.replaceChildren();
    for (const memory of data.memories) {
      const card = element('article', 'memory-card');
      card.append(element('strong', '', memory.key), element('p', '', memory.content), element('p', '', `${memory.source_kind === 'owner_note' ? 'Your note' : 'Your message'} · ${new Date(memory.source_date).toLocaleString()}`), element('blockquote', '', memory.quote));
      const edit = element('button', 'text-button', 'Correct');
      edit.onclick = () => { replaces = memory.id; key.value = memory.key; key.readOnly = true; content.value = memory.content; content.focus(); };
      const forget = element('button', 'text-button', 'Forget');
      forget.onclick = () => { forget.disabled = true; void post(`/memory/${memory.id}/forget`).then(async () => { clear(); await load(); status.textContent = 'Forgotten. Send a follow-up to continue with the updated memory.'; }).catch((e: Error) => { status.textContent = e.message; forget.disabled = false; }); };
      card.append(edit, forget); list.append(card);
    }
    status.textContent = data.memories.length ? 'Up to eight relevant facts shown.' : 'No matching facts.';
  }
  const work = workPanels.register(panel, button, { open: () => { void load().catch((e: Error) => { status.textContent = e.message; }); } });
  panel.querySelector<HTMLButtonElement>('#memory-close')!.onclick = () => work.close();
  panel.querySelector<HTMLButtonElement>('#memory-new')!.onclick = clear;
  panel.querySelector<HTMLFormElement>('#memory-search')!.onsubmit = (e) => { e.preventDefault(); void load().catch((error: Error) => { status.textContent = error.message; }); };
  form.onsubmit = (e) => { e.preventDefault(); const save = form.querySelector<HTMLButtonElement>('button')!; save.disabled = true;
    void post('/memory', { key: key.value, content: content.value, ...(replaces ? { replaces } : {}) }).then(async () => { clear(); await load(); status.textContent = 'Saved with your note as its source.'; }).catch((error: Error) => { status.textContent = error.message; }).finally(() => { save.disabled = false; });
  };
  void load().catch(() => { /* Daemon may still be starting. */ });
}
