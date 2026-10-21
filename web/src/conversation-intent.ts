import { currentView } from './view-lifetime';
import { element } from './view';
import './agent-request.css';
import './creation-mode.css';

// Mode is visible UI state. The message field always remains the owner's draft.
export function installConversationIntent(input: { focus: () => void; setPlaceholder: (text: string) => void }): (content: string) => string {
  const view = currentView();
  const url = new URL(view.url);
  let intent = url.searchParams.get('intent') ?? '';
  const item = url.searchParams.get('item');
  let reference = item && /^[a-zA-Z0-9-]{1,80}$/.test(item) ? item : '';
  const form = view.querySelector<HTMLElement>('.composer-area')!;
  const area = view.querySelector('.composer-area')!;
  const banner = element('div', 'creation-mode');
  const label = element('strong');
  const select = document.createElement('select'); select.setAttribute('aria-label', 'Panel template');
  for (const [value, text] of [['auto', 'Choose for me'], ['metric', 'Metric'], ['list', 'List'], ['chart', 'Chart'], ['agenda', 'Agenda'], ['brief', 'Brief']]) {
    const option = document.createElement('option'); option.value = value; option.textContent = text; select.append(option);
  }
  if (['metric', 'list', 'chart', 'agenda', 'brief'].includes(url.searchParams.get('template') ?? '')) select.value = url.searchParams.get('template')!;
  select.onchange = () => { url.searchParams.set('template', select.value); history.replaceState(null, '', url); };
  const close = element('button', 'icon-button', '×'); close.type = 'button'; close.setAttribute('aria-label', 'Exit creation mode');
  banner.append(label, select, close); area.prepend(banner);
  const supported = ['panel', 'app', 'edit-panel', 'edit-app'];
  const draw = (): void => {
    const active = supported.includes(intent);
    banner.hidden = !active; form.classList.toggle('is-creating', active);
    label.textContent = ({ panel: 'Creating a panel', app: 'Creating an app', 'edit-panel': 'Updating a panel', 'edit-app': 'Updating an app' } as Record<string, string>)[intent] ?? '';
    select.hidden = intent !== 'panel';
    input.setPlaceholder(active ? 'Describe what you need…' : 'Message…');
  };
  close.onclick = () => { intent = ''; reference = ''; for (const key of ['intent', 'item', 'topic', 'template']) url.searchParams.delete(key); history.replaceState(null, '', url); draw(); input.focus(); };
  draw();
  return content => {
    if (!supported.includes(intent) || content.startsWith('[Ayati request mode:')) return content;
    const mode = intent.includes('panel') ? `Home panel${select.value === 'auto' ? '' : `, ${select.value} template`}` : 'app';
    return `[Ayati request mode: ${intent.startsWith('edit') ? 'Update' : 'Create'} ${mode}${reference ? `, ID ${reference}` : ''}]\n${content}`;
  };
}
