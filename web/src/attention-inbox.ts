import { element } from './view';
import './attention-inbox.css';
export function installAttentionInbox(container: HTMLElement): (items: HTMLElement[]) => void {
  const dialog = document.createElement('dialog'); dialog.className = 'attention-inbox'; dialog.setAttribute('aria-label', 'Agent inbox');
  const header = element('div', 'panel-header'); header.append(element('h2', '', 'Needs your attention'));
  const close = element('button', 'icon-button', '×'); close.setAttribute('aria-label', 'Close inbox'); header.append(close);
  const filters = element('div', 'inbox-filters'); filters.setAttribute('aria-label', 'Attention categories');
  const list = element('div', 'home-list'); list.id = 'home-attention';
  const status = element('p', 'home-detail'); status.id = 'inbox-connection'; status.setAttribute('role', 'status');
  dialog.append(header, container.querySelector('#home-error')!, status, filters, list); container.append(dialog);
  close.onclick = () => dialog.close();
  container.querySelector<HTMLButtonElement>('#home-decisions')!.onclick = () => {
    category = current.some(item => item.dataset.category === 'Decisions') ? 'Decisions' : 'Problems';
    draw(); dialog.showModal();
  };
  container.querySelector<HTMLButtonElement>('#home-updates')!.onclick = () => {
    category = current.some(item => item.dataset.category === 'Updates') ? 'Updates' : 'Problems';
    draw(); dialog.showModal();
  };
  let current: HTMLElement[] = [], category = 'Decisions';
  const tabs = new Map<string, HTMLButtonElement>();
  for (const name of ['Decisions', 'Problems', 'Updates']) {
    const button = element('button', 'text-button', name); button.dataset.category = name; tabs.set(name, button); filters.append(button);
    button.onclick = () => { category = name; draw(); };
  }
  function draw(): void {
    const expanded = new Set([...list.querySelectorAll<HTMLDetailsElement>('[data-agent-group][open]')].map(d => d.dataset.agentGroup));
    list.replaceChildren();
    for (const [name, button] of tabs) {
      button.textContent = `${name} · ${current.filter(n => n.dataset.category === name).length}`;
      button.setAttribute('aria-pressed', String(name === category));
    }
    const groups = new Map<string, HTMLElement[]>();
    for (const item of current.filter(n => n.dataset.category === category)) {
      const agent = item.dataset.agent ?? 'Other'; groups.set(agent, [...(groups.get(agent) ?? []), item]);
    }
    for (const [agent, items] of groups) {
      const group = document.createElement('details'); group.dataset.agentGroup = agent; group.className = 'inbox-group';
      group.open = expanded.has(agent) || groups.size === 1;
      const name = items[0].querySelector('.home-agent-name')?.textContent ?? agent;
      group.append(element('summary', '', `${name} · ${items.length}`), ...items); list.append(group);
    }
    list.append(...current.filter(n => !n.dataset.category));
    if (!groups.size) list.append(element('p', 'home-empty', `No ${category.toLowerCase()} waiting.`));
  }
  return items => { current = items; draw(); };
}
