/**
 * Saved Home panels as the phone reads them (read-only in the first release,
 * D253). Types and value rules mirror web/src/panel-types.ts and
 * web/src/panel-data.ts.
 */
export type ChartVariant = 'line' | 'bar' | 'area' | 'stacked_bar' | 'donut';
export type Comparison = { label: string; value: number; unit?: string; favorable?: 'increase' | 'decrease' | 'neutral' };
export type Metric = { id: string; label: string; value: string; comparison?: Comparison };
export type Point = { label: string; value: number };
export type Series = { name: string; points: Point[] };
export type ProgressItem = { id: string; label: string; actual: number; target: number; minimum?: number; unit?: string };
export type Section =
  | { kind: 'text'; heading: string; body: string }
  | { kind: 'metric'; label: string; value: string; comparison?: Comparison }
  | { kind: 'metrics'; items: Metric[] }
  | { kind: 'table'; columns: string[]; rows: string[][]; column_types?: ('text' | 'number')[] }
  | { kind: 'chart'; title: string; points?: Point[]; variant?: ChartVariant; series?: Series[]; unit?: string }
  | { kind: 'collection'; items: { id: string; title: string; description?: string; status?: string }[] }
  | { kind: 'progress'; items: ProgressItem[] }
  | { kind: 'timeline'; events: { id: string; at_ms: number; label: string; description?: string }[] }
  | { kind: 'pipeline'; stages: { id: string; label: string; count: number; value?: string }[] };
export type ChartSection = Extract<Section, { kind: 'chart' }>;
export type PanelDefinition = { title: string; purpose: string; sections: Section[] };

export type Snapshot = {
  revision: number;
  available: boolean;
  definition?: PanelDefinition;
  saved_ms?: number;
  chat?: string;
  thread?: string | null;
  task?: string;
  checked_ms?: number;
  what_changed?: string | null;
  from_source?: boolean;
};
export type Maintenance = {
  enabled: boolean;
  status: string;
  interval_seconds: number | null;
  stale_seconds: number;
  checked_ms: number | null;
  saved_ms: number | null;
  next_ms: number | null;
  from_source?: boolean;
};
export type Panel = {
  id: string;
  revision: number;
  agent_name: string | null;
  agent?: string | null;
  active: Snapshot | null;
  maintenance?: Maintenance | null;
  shared?: boolean;
  owner_name?: string | null;
};
export type HomeLayout = { revision: number; order: string[]; wide: string[]; hidden: string[] };
/** `GET …/home-panels`. */
export type PanelsPage = { available: boolean; panels: Panel[]; layout: HomeLayout };

/** A panel with something to show, in the person's web order; hidden ones left out. */
export type ShownPanel = Panel & { definition: PanelDefinition; saved_ms?: number };

export function shownPanels(page: PanelsPage): ShownPanel[] {
  const hidden = new Set(page.layout.hidden);
  const position = new Map(page.layout.order.map((id, index) => [id, index]));
  return page.panels
    .filter(panel => !hidden.has(panel.id) && panel.active?.available && panel.active.definition?.sections.length)
    .map(panel => ({ ...panel, definition: panel.active!.definition!, saved_ms: panel.active!.saved_ms }))
    .sort((a, b) => (position.get(a.id) ?? Infinity) - (position.get(b.id) ?? Infinity));
}

function withUnit(number: string, unit: string) {
  return unit === '%' ? `${number}%` : unit ? `${number} ${unit}` : number;
}

export function formatValue(value: number, unit = ''): string {
  const compact = value !== 0 && (Math.abs(value) < 0.01 || Math.abs(value) >= 1e15);
  const number = compact ? value.toExponential(2) : new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(value);
  return withUnit(number, unit);
}

export function comparisonView(value: Comparison): { sentiment: 'favorable' | 'unfavorable' | 'neutral'; text: string } {
  const favorable = value.favorable ?? 'neutral';
  const sentiment =
    !value.value || favorable === 'neutral' ? 'neutral' : value.value > 0 === (favorable === 'increase') ? 'favorable' : 'unfavorable';
  return { sentiment, text: `${value.value > 0 ? '+' : ''}${formatValue(value.value, value.unit)} · ${value.label}` };
}

export function progressView(value: ProgressItem): { percentage: string; width: number } {
  const minimum = value.minimum ?? 0;
  const scale = Math.max(Math.abs(value.actual), Math.abs(value.target), Math.abs(minimum), Number.MIN_VALUE);
  const ratio = (value.actual / scale - minimum / scale) / (value.target / scale - minimum / scale);
  return {
    percentage: Number.isFinite(ratio * 100) ? `${formatValue(ratio * 100)}%` : value.actual > value.target ? 'Above target' : 'Below range',
    width: Math.min(100, Math.max(0, Number.isNaN(ratio) ? 0 : ratio * 100)),
  };
}

/** A chart's series, whether it was saved as one list of points or several series. */
export function chartSeries(section: ChartSection): Series[] {
  if (section.series?.length) return section.series;
  return section.points?.length ? [{ name: section.title, points: section.points }] : [];
}

/** Rows for "Show the numbers": one per label, one column per series. */
export function chartTable(section: ChartSection): { columns: string[]; rows: string[][] } {
  const series = chartSeries(section);
  const labels: string[] = [];
  for (const one of series) for (const point of one.points) if (!labels.includes(point.label)) labels.push(point.label);
  return {
    columns: ['', ...series.map(one => one.name)],
    rows: labels.map(label => [
      label,
      ...series.map(one => {
        const point = one.points.find(p => p.label === label);
        return point ? formatValue(point.value, section.unit) : '';
      }),
    ]),
  };
}
