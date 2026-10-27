// Compatibility renderer for the adopted local-owner Home subtree.
// Managed personal Home uses native React components with the same data/geometry.
import { element } from './view';
import type { PanelDefinition } from './panel-types';
import { renderSection } from './panel-compat-sections';
import './panel-visualizations.css';
export type { PanelDefinition, Section } from './panel-types';

export function renderPanel(definition: PanelDefinition): HTMLElement {
  const view = element('div', 'panel-snapshot');
  view.append(element('h3', '', definition.title), element('p', 'panel-purpose', definition.purpose));
  for (const section of definition.sections) {
    const block = element('section', `panel-block panel-${section.kind}`);
    renderSection(block, section);
    view.append(block);
  }
  return view;
}
