export type PanelFamily = 'metric' | 'chart' | 'table' | 'collection' | 'progress' | 'timeline' | 'text';
export type PanelTemplate = PanelFamily | 'list' | 'agenda' | 'brief';
export type ChartVariant = 'line' | 'bar' | 'area' | 'stacked_bar' | 'donut';
export type Comparison = {
  label: string;
  value: number;
  unit?: string;
  favorable?: 'increase' | 'decrease' | 'neutral';
};
export type Metric = { id: string; label: string; value: string; comparison?: Comparison };
export type Point = { label: string; value: number };
export type Series = { name: string; points: Point[] };
export type Progress = { id: string; label: string; actual: number; target: number; minimum?: number; unit?: string };
export type Section =
  | { kind: 'text'; heading: string; body: string }
  | { kind: 'metric'; label: string; value: string; comparison?: Comparison }
  | { kind: 'metrics'; items: Metric[] }
  | { kind: 'table'; columns: string[]; rows: string[][]; column_types?: ('text' | 'number')[] }
  | { kind: 'chart'; title: string; points?: Point[]; variant?: ChartVariant; series?: Series[]; unit?: string }
  | { kind: 'collection'; items: { id: string; title: string; description?: string; status?: string }[] }
  | { kind: 'progress'; items: Progress[] }
  | { kind: 'timeline'; events: { id: string; at_ms: number; label: string; description?: string }[] };
export type ChartSection = Extract<Section, { kind: 'chart' }>;
export type TableSection = Extract<Section, { kind: 'table' }>;
export type PanelDefinition = { title: string; purpose: string; template?: PanelTemplate; sections: Section[] };

export const PANEL_FAMILIES: { id: PanelFamily; label: string; description: string }[] = [
  { id: 'metric', label: 'Metric', description: 'One value or a small group, with an optional comparison.' },
  { id: 'chart', label: 'Chart', description: 'Patterns over time, category comparisons or parts of a whole.' },
  { id: 'table', label: 'Table', description: 'Compare the same fields across a bounded set of records.' },
  { id: 'collection', label: 'Collection', description: 'Scan items with descriptions and optional status labels.' },
  { id: 'progress', label: 'Progress', description: 'Actual values against a defined target or range.' },
  { id: 'timeline', label: 'Timeline', description: 'Events arranged by when they happened.' },
  { id: 'text', label: 'Text / summary', description: 'A heading and written context or findings.' },
];
export const CHART_VARIANTS: { id: ChartVariant; label: string }[] = [
  { id: 'line', label: 'Line' }, { id: 'bar', label: 'Bar' }, { id: 'area', label: 'Area' },
  { id: 'stacked_bar', label: 'Stacked bar' }, { id: 'donut', label: 'Donut' },
];
