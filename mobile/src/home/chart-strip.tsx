/**
 * A chart drawn small enough for a phone: lines, areas, bars, stacked bars or
 * a donut. Values are read in "Show the numbers" on the panel's own page.
 */
import { chartSeries, type ChartSection } from '@cuckoo/shared';
import Svg, { Circle, Path, Rect } from 'react-native-svg';
import { useTokens } from '../theme/use-tokens';

// Series after the first use the muted and status colours, never a new palette.
function palette(t: ReturnType<typeof useTokens>): string[] {
  return [t.text, t.working, t.success, t.warning, t.muted];
}

export function ChartStrip({ section, width, height }: { section: ChartSection; width: number; height: number }) {
  const t = useTokens();
  const colours = palette(t);
  const series = chartSeries(section);
  const labels = [...new Set(series.flatMap(one => one.points.map(point => point.label)))];
  if (!labels.length || width <= 0) return null;
  const value = (name: number, label: string) => series[name].points.find(point => point.label === label)?.value ?? 0;
  const variant = section.variant ?? 'line';

  if (variant === 'donut') {
    const slices = labels.map(label => Math.max(0, value(0, label)));
    const total = slices.reduce((sum, n) => sum + n, 0) || 1;
    const radius = Math.min(width, height) / 2 - 6;
    const centre = { x: width / 2, y: height / 2 };
    let angle = -Math.PI / 2;
    return (
      <Svg width={width} height={height}>
        <Circle cx={centre.x} cy={centre.y} r={radius} stroke={t.border} strokeWidth={10} fill="none" />
        {slices.map((slice, index) => {
          const sweep = (slice / total) * Math.PI * 2;
          const start = angle;
          angle += sweep;
          if (!sweep) return null;
          const end = angle - 0.0001;
          const large = sweep > Math.PI ? 1 : 0;
          const point = (a: number) => `${centre.x + radius * Math.cos(a)} ${centre.y + radius * Math.sin(a)}`;
          return (
            <Path key={labels[index]} d={`M ${point(start)} A ${radius} ${radius} 0 ${large} 1 ${point(end)}`}
              stroke={colours[index % colours.length]} strokeWidth={10} fill="none" />
          );
        })}
      </Svg>
    );
  }

  const stacked = variant === 'stacked_bar';
  const totals = labels.map(label => series.reduce((sum, _, index) => sum + value(index, label), 0));
  const all = stacked ? totals : series.flatMap((_, index) => labels.map(label => value(index, label)));
  const max = Math.max(...all, 0);
  const min = Math.min(...all, 0);
  const span = max - min || 1;
  const y = (n: number) => height - 4 - ((n - min) / span) * (height - 8);
  const step = width / labels.length;

  if (variant === 'bar' || stacked) {
    const bars = stacked ? 1 : series.length;
    const barWidth = Math.max(2, (step * 0.7) / bars);
    return (
      <Svg width={width} height={height}>
        {labels.map((label, li) => {
          let base = 0;
          return series.map((_, si) => {
            const n = value(si, label);
            const top = stacked ? y(base + n) : y(Math.max(n, 0));
            const bottom = stacked ? y(base) : y(Math.min(n, 0));
            base += n;
            const x = li * step + step * 0.15 + (stacked ? 0 : si * barWidth);
            return <Rect key={`${label}/${si}`} x={x} y={top} width={barWidth} height={Math.max(1, bottom - top)} rx={2} fill={colours[si % colours.length]} />;
          });
        })}
      </Svg>
    );
  }

  return (
    <Svg width={width} height={height}>
      {series.map((_, si) => {
        const points = labels.map((label, li) => `${li * step + step / 2},${y(value(si, label))}`);
        const line = `M ${points.join(' L ')}`;
        const colour = colours[si % colours.length];
        return variant === 'area' ? (
          <Path key={si} d={`${line} L ${(labels.length - 0.5) * step},${y(min)} L ${step / 2},${y(min)} Z`}
            fill={colour} fillOpacity={0.15} stroke={colour} strokeWidth={2} />
        ) : (
          <Path key={si} d={line} fill="none" stroke={colour} strokeWidth={2} strokeLinejoin="round" />
        );
      })}
    </Svg>
  );
}
