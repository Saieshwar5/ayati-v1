import { api } from './types';
import { element } from './view';
import './agent-request.css';

export type Creation = 'panel' | 'app';
export function conversationUrl(agent: string, intent?: string, id?: string): string {
  const params = new URLSearchParams({ agent });
  if (intent) params.set('intent', intent);
  if (id) params.set('item', id);
  return `/?${params}`;
}

// Selection only navigates: work starts when the owner sends the conversation draft.
export function chooseAgent(kind: Creation, topic?: string): void {
  const dialog = document.createElement('dialog'); dialog.className = 'creation-chooser';
  dialog.setAttribute('aria-labelledby', 'creation-title');
  const header = element('div', 'creation-heading');
  const close = element('button', 'icon-button', '×'); close.setAttribute('aria-label', 'Close agent chooser');
  const heading = element('h2', '', `Who should ${kind === 'panel' ? 'make your panel' : 'build your app'}?`); heading.id = 'creation-title';
  header.append(heading, close);
  const status = element('p', 'creation-description', 'Choose an agent. You’ll describe what you need in their conversation.');
  const list = element('div', 'creation-contacts'); list.setAttribute('aria-live', 'polite'); list.textContent = 'Loading agents…';
  dialog.append(header, status, list); document.body.append(dialog); dialog.showModal();
  close.onclick = () => dialog.close(); dialog.onclose = () => dialog.remove();
  void api<{ agents: { profile: { id: string; name: string; role: string; state: string } }[] }>('/agents').then(data => {
    if (!dialog.open) return;
    list.replaceChildren();
    const agents = data.agents.sort((a, b) => Number(b.profile.id === 'local-agent') - Number(a.profile.id === 'local-agent'));
    for (const { profile } of agents) {
      const row = element('a', 'creation-contact');
      row.href = conversationUrl(profile.id, kind) + (topic ? `&topic=${encodeURIComponent(topic)}` : '');
      const avatar = element('span', 'creation-avatar', profile.id === 'local-agent' ? 'a.' : profile.name.slice(0, 1));
      const copy = element('span');
      copy.append(element('strong', '', profile.name), element('small', '', profile.id === 'local-agent' ? 'Your guide · A good place to start' : profile.role || 'Your agent'));
      row.append(avatar, copy, element('span', 'creation-arrow', '↗'));
      if (profile.state !== 'ready') { row.removeAttribute('href'); row.setAttribute('aria-disabled', 'true'); copy.append(element('small', '', 'Still getting ready')); }
      list.append(row);
    }
    if (!agents.length) list.textContent = 'Agents are unavailable. Check Settings and try again.';
  }).catch(() => { list.textContent = 'Could not load agents. Close and try again.'; });
}
