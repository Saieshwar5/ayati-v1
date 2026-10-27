import { currentView } from './view-lifetime';
import { element } from './view';
import { PANEL_FAMILIES, CHART_VARIANTS } from './panel-types';
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
  const select = document.createElement('select'); select.setAttribute('aria-label', 'Panel type');
  for (const [value, text] of [['auto', 'Choose for me'], ...PANEL_FAMILIES.map(f => [f.id, f.label])]) {
    const option = document.createElement('option'); option.value = value; option.textContent = text; select.append(option);
  }
  const previous = url.searchParams.get('template') ?? '';
  const alias = ({ list: 'table', agenda: 'table', brief: 'text' } as Record<string, string>)[previous] ?? previous;
  if (PANEL_FAMILIES.some(f => f.id === alias)) select.value = alias;
  const variant = document.createElement('select'); variant.setAttribute('aria-label', 'Chart style');
  for (const item of CHART_VARIANTS) {
    const option = document.createElement('option'); option.value = item.id; option.textContent = item.label; variant.append(option);
  }
  if (CHART_VARIANTS.some(v => v.id === url.searchParams.get('variant'))) variant.value = url.searchParams.get('variant')!;
  select.onchange = () => { url.searchParams.set('template', select.value); history.replaceState(null, '', url); draw(); };
  variant.onchange = () => { url.searchParams.set('variant', variant.value); history.replaceState(null, '', url); };
  const close = element('button', 'icon-button', '×'); close.type = 'button'; close.setAttribute('aria-label', 'Exit creation mode');
  banner.append(label, select, variant, close); area.prepend(banner);
  const supported = ['panel', 'app', 'edit-panel', 'edit-app'];
  const draw = (): void => {
    const active = supported.includes(intent);
    banner.hidden = !active; form.classList.toggle('is-creating', active);
    label.textContent = ({ panel: 'Creating a panel', app: 'Creating an app', 'edit-panel': 'Updating a panel', 'edit-app': 'Updating an app' } as Record<string, string>)[intent] ?? '';
    select.hidden = intent !== 'panel';
    variant.hidden = intent !== 'panel' || select.value !== 'chart';
    input.setPlaceholder(active ? 'Describe what you need…' : 'Message…');
  };
  close.onclick = () => { intent = ''; reference = ''; for (const key of ['intent', 'item', 'topic', 'template', 'variant']) url.searchParams.delete(key); history.replaceState(null, '', url); draw(); input.focus(); };
  draw();
  return content => {
    if (!supported.includes(intent) || content.startsWith('[Ayati request mode:')) return content;
    const mode = intent.includes('panel') ? `Home panel${select.value === 'auto' ? '' : `, ${select.value} template`}${select.value === 'chart' ? `, chart variant ${variant.value}` : ''}` : 'app';
    return `[Ayati request mode: ${intent.startsWith('edit') ? 'Update' : 'Create'} ${mode}${reference ? `, ID ${reference}` : ''}]\n${content}`;
  };
}
