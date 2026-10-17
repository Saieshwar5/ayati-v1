import { element } from './view';
export type Section = { kind: 'text'; heading: string; body: string } | { kind: 'metric'; label: string; value: string } | { kind: 'table'; columns: string[]; rows: string[][] } | { kind: 'chart'; title: string; points: { label: string; value: number }[] };
export type PanelDefinition = { title: string; purpose: string; sections: Section[] };

// Agent content becomes text and bounded chart coordinates, never executable HTML.
export function renderPanel(definition: PanelDefinition): HTMLElement {
  const view = element('div', 'panel-snapshot');
  view.append(element('h3', '', definition.title), element('p', 'panel-purpose', definition.purpose));
  for (const section of definition.sections) {
    const block = element('section', `panel-block panel-${section.kind}`);
    if (section.kind === 'text') block.append(element('h4', '', section.heading), element('p', 'panel-text', section.body));
    else if (section.kind === 'metric') block.append(element('p', '', section.label), element('strong', 'panel-metric-value', section.value));
    else if (section.kind === 'table') {
      const wrapper = element('div', 'panel-table-scroll'), table = document.createElement('table');
      const head = document.createElement('thead'), row = document.createElement('tr');
      for (const title of section.columns) row.append(element('th', '', title)); head.append(row);
      const body = document.createElement('tbody');
      for (const values of section.rows) { const row = document.createElement('tr'); for (const value of values) row.append(element('td', '', value)); body.append(row); }
      table.append(head, body); wrapper.append(table); block.append(wrapper);
    } else {
      block.append(element('h4', '', section.title));
      if (section.points.length <= 8 && section.points.every(point => point.value >= 0)) {
        const bars = element('div', 'panel-bars');
        const high = Math.max(...section.points.map(point => point.value), 1);
        for (const point of section.points) {
          const column = element('div', 'panel-bar-column');
          const track = element('div', 'panel-bar-track'), bar = element('div', 'panel-bar');
          bar.style.height = `${point.value / high * 100}%`;
          track.append(bar); column.append(track, element('span', '', point.label));
          column.setAttribute('aria-label', `${point.label}: ${point.value}`); bars.append(column);
        }
        block.append(bars);
      } else {
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.setAttribute('viewBox', '0 0 500 180'); svg.setAttribute('role', 'img'); svg.setAttribute('aria-label', section.title);
      const values = section.points.map(p => p.value), low = Math.min(...values), high = Math.max(...values);
      const line = document.createElementNS(svg.namespaceURI, 'polyline');
      line.setAttribute('points', section.points.map((p, i) => `${20 + i * 460 / Math.max(1, section.points.length - 1)},${160 - (p.value - low) * 140 / (high - low || 1)}`).join(' '));
      line.setAttribute('fill', 'none'); line.setAttribute('stroke', 'currentColor'); line.setAttribute('stroke-width', '3'); svg.append(line); block.append(svg);
      }
      const detail = document.createElement('details'); detail.append(element('summary', '', 'Chart values'));
      for (const point of section.points) detail.append(element('p', '', `${point.label}: ${point.value}`)); block.append(detail);
    }
    view.append(block);
  }
  return view;
}
