import type { ChartSection } from './panel-types';

export function chartData(section: ChartSection) {
  const series = section.series?.length ? section.series : [{ name: section.title, points: section.points ?? [] }];
  const variant = section.variant ?? 'line';
  const labels = series[0]?.points.map(p => p.label) ?? [];
  const values = series.flatMap(s => s.points.map(p => p.value));
  const totals = labels.map((_, i) => series.reduce((sum, s) => sum + s.points[i].value, 0));
  const low = variant === 'stacked_bar' ? 0 : Math.min(0, ...values);
  const high = Math.max(0, ...(variant === 'stacked_bar' ? totals : values)) || 1;
  const scale = Math.max(Math.abs(low), Math.abs(high), Number.MIN_VALUE);
  const y = (value: number) => 185 - (value / scale - low / scale) / (high / scale - low / scale || 1) * 170;
  const bar = variant === 'bar' || variant === 'stacked_bar';
  const step = 440 / Math.max(1, labels.length);
  const x = (i: number) => bar ? 48 + (i + .5) * step : labels.length === 1 ? 268 : 48 + i * 440 / Math.max(1, labels.length - 1);
  const lines = series.map((s, index) => ({ index, points: s.points.map((p, i) => `${x(i)},${y(p.value)}`).join(' ') }));
  const dots = series.flatMap((s, index) => s.points.map((p, i) => ({ index, key: `${index}.${i}`, x: x(i), y: y(p.value), label: p.label, value: p.value })));
  const areas = lines.map(line => ({ ...line, points: `${x(0)},${y(0)} ${line.points} ${x(labels.length-1)},${y(0)}` }));
  const bars = series.flatMap((s, index) => s.points.map((p, i) => {
    const before = variant === 'stacked_bar' ? series.slice(0, index).reduce((sum, s) => sum + s.points[i].value, 0) : 0;
    const after = before + p.value;
    const width = variant === 'stacked_bar' ? step * .7 : step * .7 / series.length;
    return { index, key: `${index}.${i}`, x: x(i)-step*.35+(variant === 'bar' ? width*index : 0),
      y: Math.min(y(before), y(after)), width, height: Math.abs(y(after)-y(before)) };
  }));
  const total = series[0]?.points.reduce((sum, p) => sum + p.value, 0) ?? 0;
  let offset = 0;
  const slices = (series[0]?.points ?? []).map((p, index) => {
    const fraction = total > 0 ? p.value / total : 0;
    const value = { index, label: p.label, value: p.value, fraction, offset };
    offset += fraction;
    return value;
  });
  const tickCount = Math.min(4, labels.length);
  const ticks = new Set(Array.from({ length: tickCount }, (_, i) => Math.round(i * (labels.length - 1) / Math.max(1, tickCount - 1))));
  return { series, variant, labels, low, high, lines, areas, bars, slices, dots,
    zero: y(0), empty: !labels.length || (variant === 'donut' && !(total > 0)),
    ticks: labels.flatMap((label, i) => ticks.has(i) ? [{ label, x: x(i) }] : []),
  };
}
export const seriesColor = (index: number) => `var(--panel-series-${index % 4 + 1})`;
