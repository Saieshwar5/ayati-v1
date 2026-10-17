import { element } from './view';
import type { PanelDefinition } from './panel-renderer';
type Layout = { order: string[]; wide: string[] };
export function panelTemplate(definition?: PanelDefinition | null): string {
  if (definition?.sections.length !== 1) return 'legacy';
  return definition.template ?? ({ metric: 'metric', table: 'list', chart: 'chart', text: 'brief' }[definition.sections[0].kind]);
}
export function loadLayout(): Layout {
  try {
    const value = JSON.parse(localStorage.getItem('ayati.home.layout') ?? 'null');
    if (value && Array.isArray(value.order) && Array.isArray(value.wide)) return { order: [...new Set<string>(value.order.filter((v: unknown) => typeof v === 'string'))], wide: value.wide.filter((v: unknown) => typeof v === 'string') };
  } catch { /* Layout remains usable without storage. */ }
  return { order: [], wide: [] };
}
export function arrangement(card: HTMLElement, id: string, ids: string[], layout: Layout, template: string, redraw: () => void): void {
  const save = (): void => { try { localStorage.setItem('ayati.home.layout', JSON.stringify(layout)); } catch { /* In-memory placement still works. */ } redraw();
    [...document.querySelectorAll<HTMLElement>('[data-panel]')].find(n => n.dataset.panel === id)?.querySelector<HTMLButtonElement>('.panel-drag')?.focus();
  };
  const move = (target: number): void => { const next = ids.filter(item => item !== id); next.splice(Math.max(0, target), 0, id); layout.order = next; save(); };
  const index = ids.indexOf(id), controls = element('div', 'panel-arrange');
  const handle = element('button', 'text-button panel-drag', '⠿'); handle.draggable = true; handle.setAttribute('aria-label', 'Drag panel to reorder');
  handle.ondragstart = event => { event.dataTransfer!.setData('text/ayati-panel', id); event.dataTransfer!.effectAllowed = 'move'; };
  controls.append(handle);
  for (const [label, target, disabled] of [['Top', 0, index === 0], ['Move earlier', index - 1, index === 0], ['Move later', index + 1, index === ids.length - 1]] as const) {
    const button = element('button', 'text-button', label); button.disabled = disabled; if (disabled) button.dataset.noEnable = 'true';
    button.onclick = () => move(target); controls.append(button);
  }
  if (template !== 'metric') {
    const size = element('button', 'text-button', layout.wide.includes(id) ? 'Standard width' : 'Make wider');
    size.onclick = () => { layout.wide = layout.wide.includes(id) ? layout.wide.filter(v => v !== id) : [...layout.wide, id]; save(); }; controls.append(size);
  }
  card.ondragover = event => { if (event.dataTransfer?.types.includes('text/ayati-panel')) { event.preventDefault(); card.classList.add('panel-drop-target'); } };
  card.ondragleave = () => card.classList.remove('panel-drop-target');
  card.ondrop = event => {
    event.preventDefault(); card.classList.remove('panel-drop-target');
    const source = event.dataTransfer?.getData('text/ayati-panel'); if (!source || !ids.includes(source) || source === id) return;
    layout.order = ids.filter(v => v !== source); layout.order.splice(index, 0, source); save();
  };
  card.prepend(controls);
}
