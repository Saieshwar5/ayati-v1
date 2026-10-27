import type { Comparison, Progress, TableSection } from './panel-types';

function withUnit(number: string, unit: string) {
  return unit === '%' ? `${number}%` : unit ? `${number} ${unit}` : number;
}
export function formatValue(value: number, unit = ''): string {
  const compact = value !== 0 && (Math.abs(value) < .01 || Math.abs(value) >= 1e15);
  const number = compact ? value.toExponential(2)
    : new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(value);
  return withUnit(number, unit);
}
export function exactValue(value: number, unit = ''): string {
  return withUnit(String(value), unit);
}
export function comparisonView(value: Comparison) {
  const favorable = value.favorable ?? 'neutral';
  const sentiment = !value.value || favorable === 'neutral' ? 'neutral'
    : (value.value > 0) === (favorable === 'increase') ? 'favorable' : 'unfavorable';
  return { sentiment, text: `${value.value > 0 ? '+' : ''}${formatValue(value.value, value.unit)} · ${value.label}` };
}
export function progressView(value: Progress) {
  const minimum = value.minimum ?? 0;
  const scale = Math.max(Math.abs(value.actual), Math.abs(value.target), Math.abs(minimum), Number.MIN_VALUE);
  const ratio = (value.actual / scale - minimum / scale) / (value.target / scale - minimum / scale);
  return {
    percentage: Number.isFinite(ratio * 100) ? `${formatValue(ratio * 100)}%` : value.actual > value.target ? 'Above target' : 'Below range',
    width: Math.min(100, Math.max(0, Number.isNaN(ratio) ? 0 : ratio * 100)),
  };
}
export function sortedRows(section: TableSection, sort?: { column: number; descending: boolean }) {
  const rows = section.rows.map((values, index) => ({ values, index }));
  if (!sort || sort.column >= section.columns.length) return rows;
  const numeric = section.column_types?.[sort.column] === 'number';
  return rows.sort((a, b) => {
    const left = a.values[sort.column], right = b.values[sort.column];
    if (!left.trim() || !right.trim()) return left.trim() ? -1 : right.trim() ? 1 : a.index - b.index;
    const result = numeric ? Number(left) - Number(right) : left.localeCompare(right);
    return (sort.descending ? -result : result) || a.index - b.index;
  });
}
