import { element } from './view';
import type { Comparison, Section, TableSection } from './panel-types';
import { comparisonView, formatValue, progressView, sortedRows } from './panel-data';
import { renderChart } from './panel-compat-chart';

function comparison(value?: Comparison) {
  const node = element('p', 'panel-comparison');
  if (value) { const view = comparisonView(value); node.textContent = view.text; node.dataset.sentiment = view.sentiment; }
  return node;
}
function table(block: HTMLElement, section: TableSection) {
  const wrapper = element('div', 'panel-table-scroll'), table = element('table');
  const head = element('thead'), row = element('tr'), body = element('tbody');
  let sort: { column: number; descending: boolean } | undefined;
  function draw() {
    body.replaceChildren();
    for (const item of sortedRows(section, sort)) {
      const row = element('tr'); for (const cell of item.values) row.append(element('td', '', cell)); body.append(row);
    }
    for (const [i, cell] of [...row.children].entries()) cell.setAttribute('aria-sort', sort?.column === i ? sort.descending ? 'descending' : 'ascending' : 'none');
  }
  for (const [i, name] of section.columns.entries()) {
    const cell = element('th'), button = element('button', '', name); cell.setAttribute('scope', 'col'); button.type = 'button';
    button.onclick = () => { sort = { column: i, descending: sort?.column === i && !sort.descending }; draw(); };
    cell.append(button); row.append(cell);
  }
  head.append(row); table.append(head, body); wrapper.append(table);
  if (!section.rows.length) block.append(element('p', 'panel-empty', 'No records yet.'));
  block.append(wrapper); draw();
}
export function renderSection(block: HTMLElement, section: Section) {
  switch (section.kind) {
    case 'text': block.append(element('h4', '', section.heading), element('p', 'panel-text', section.body)); break;
    case 'metric': block.append(element('p', '', section.label), element('strong', 'panel-metric-value', section.value), comparison(section.comparison)); break;
    case 'metrics': {
      const group = element('div', 'panel-metric-group');
      for (const item of section.items) {
        const value = element('div'); value.append(element('p', '', item.label), element('strong', 'panel-metric-value', item.value), comparison(item.comparison)); group.append(value);
      }
      block.append(group); break;
    }
    case 'table': table(block, section); break;
    case 'chart': renderChart(block, section); break;
    case 'collection': {
      if (!section.items.length) { block.append(element('p', 'panel-empty', 'No items yet.')); break; }
      const list = element('ul', 'panel-collection');
      for (const item of section.items) {
        const li = element('li'), text = element('div'); text.append(element('strong', '', item.title));
        if (item.description) text.append(element('p', '', item.description)); li.append(text);
        if (item.status) li.append(element('span', 'panel-item-status', item.status)); list.append(li);
      }
      block.append(list); break;
    }
    case 'progress': {
      const group = element('div', 'panel-progress-group');
      for (const item of section.items) {
        const view = progressView(item), value = element('div'), track = element('div', 'panel-progress-track');
        const description = `${formatValue(item.actual, item.unit)} actual · ${formatValue(item.target, item.unit)} target`;
        for (const [name, data] of Object.entries({ role: 'progressbar', 'aria-label': item.label, 'aria-valuemin': 0, 'aria-valuemax': 100, 'aria-valuenow': view.width, 'aria-valuetext': description })) track.setAttribute(name, String(data));
        const fill = element('span'); fill.style.width = `${view.width}%`; track.append(fill);
        value.append(element('p', '', item.label), element('strong', 'panel-progress-value', view.percentage), track, element('p', 'panel-progress-description', description));
        if (item.minimum) value.append(element('small', '', `Range starts at ${formatValue(item.minimum, item.unit)}`)); group.append(value);
      }
      block.append(group); break;
    }
    case 'timeline': {
      if (!section.events.length) { block.append(element('p', 'panel-empty', 'No events yet.')); break; }
      const list = element('ol', 'panel-timeline');
      for (const event of [...section.events].sort((a, b) => b.at_ms-a.at_ms)) {
        const li = element('li'), time = element('time', '', new Date(event.at_ms).toLocaleString()), text = element('div');
        time.setAttribute('datetime', new Date(event.at_ms).toISOString()); text.append(element('strong', '', event.label));
        if (event.description) text.append(element('p', '', event.description)); li.append(time, text); list.append(li);
      }
      block.append(list); break;
    }
  }
}
