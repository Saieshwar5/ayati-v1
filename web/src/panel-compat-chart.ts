import { element } from './view';
import type { ChartSection } from './panel-types';
import { chartData, seriesColor } from './panel-chart';
import { exactValue, formatValue } from './panel-data';

function vector(kind: string, attributes: Record<string, string | number>) {
  const node = document.createElementNS('http://www.w3.org/2000/svg', kind);
  for (const [name, value] of Object.entries(attributes)) node.setAttribute(name, String(value));
  return node;
}
export function renderChart(block: HTMLElement, section: ChartSection) {
  const data = chartData(section), donut = data.variant === 'donut';
  block.append(element('h4', '', section.title));
  if (donut ? section.unit : !data.empty) block.append(element('p', 'panel-chart-unit', donut ? section.unit ?? '' : `Range: ${formatValue(data.low)} – ${formatValue(data.high, section.unit)}`));
  if (data.empty) block.append(element('p', 'panel-empty', donut ? 'No positive values to plot.' : 'No chart data yet.'));
  else {
    const svg = vector('svg', { viewBox: '0 0 500 220', role: 'img', 'aria-label': `${section.title} — ${data.variant.replace('_', ' ')}` });
    if (donut) {
      for (const slice of data.slices) svg.append(vector('circle', {
        cx: 250, cy: 105, r: 72, fill: 'none', stroke: seriesColor(slice.index), 'stroke-width': 28,
        pathLength: 100, 'stroke-dasharray': `${slice.fraction * 100} ${100-slice.fraction*100}`,
        'stroke-dashoffset': -slice.offset*100, transform: 'rotate(-90 250 105)',
      }));
    } else {
      svg.append(vector('line', { x1: 48, x2: 488, y1: data.zero, y2: data.zero, class: 'panel-chart-axis' }));
      if (data.variant === 'area') for (const area of data.areas) svg.append(vector('polygon', {
        points: area.points, fill: seriesColor(area.index), 'fill-opacity': .15,
      }));
      if (data.variant === 'line' || data.variant === 'area') for (const line of data.lines) svg.append(vector('polyline', {
        points: line.points, fill: 'none', stroke: seriesColor(line.index), 'stroke-width': 3,
      }));
      if (data.variant === 'line' || data.variant === 'area') for (const dot of data.dots) {
        const circle = vector('circle', { cx: dot.x, cy: dot.y, r: 3, fill: seriesColor(dot.index) });
        const title = vector('title', {}); title.textContent = `${dot.label}: ${exactValue(dot.value, section.unit)}`;
        circle.append(title); svg.append(circle);
      }
      if (data.variant === 'bar' || data.variant === 'stacked_bar') for (const bar of data.bars) svg.append(vector('rect', {
        x: bar.x, y: bar.y, width: bar.width, height: bar.height, fill: seriesColor(bar.index), rx: 2,
      }));
      for (const [index, tick] of data.ticks.entries()) {
        const text = vector('text', { x: tick.x, y: 208, 'text-anchor': index === 0 ? 'start' : tick.x > 450 ? 'end' : 'middle', class: 'panel-chart-label' });
        const title = vector('title', {}); title.textContent = tick.label;
        text.append(title, document.createTextNode(tick.label.length > 7 ? `${tick.label.slice(0, 6)}…` : tick.label)); svg.append(text);
      }
    }
    block.append(svg);
    const legend = element('ul', 'panel-chart-legend'); legend.setAttribute('aria-label', 'Chart legend');
    const items = donut ? data.slices.map(s => ({ name: `${s.label}: ${formatValue(s.value, section.unit)}`, index: s.index }))
      : data.series.map((s, index) => ({ name: s.name, index }));
    for (const item of items) {
      const li = element('li'), color = element('span'); color.style.background = seriesColor(item.index); color.setAttribute('aria-hidden', 'true');
      li.append(color, document.createTextNode(item.name)); legend.append(li);
    }
    block.append(legend);
  }
  const details = element('details'), wrapper = element('div', 'panel-table-scroll'), table = element('table');
  const caption = element('caption', '', `${section.title}${section.unit ? ` (${section.unit})` : ''}`);
  const head = element('thead'), header = element('tr');
  for (const name of ['Label', ...data.series.map(s => s.name)]) {
    const cell = element('th', '', name); cell.setAttribute('scope', 'col'); header.append(cell);
  }
  head.append(header); const body = element('tbody');
  for (const [i, label] of data.labels.entries()) {
    const row = element('tr'), cell = element('th', '', label); cell.setAttribute('scope', 'row'); row.append(cell);
    for (const series of data.series) row.append(element('td', '', exactValue(series.points[i].value, section.unit)));
    body.append(row);
  }
  table.append(caption, head, body); wrapper.append(table); details.append(element('summary', '', 'Chart values'), wrapper); block.append(details);
}
