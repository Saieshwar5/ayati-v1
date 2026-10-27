import { chartData, seriesColor } from '../../panel-chart';
import { exactValue, formatValue } from '../../panel-data';
import type { ChartSection } from '../../panel-types';

export function PanelChart({ section }: { section: ChartSection }) {
  const data = chartData(section);
  const donut = data.variant === 'donut';
  return <>
    <h4>{section.title}</h4>
    {(donut ? section.unit : !data.empty) && <p className="panel-chart-unit">{donut ? section.unit : `Range: ${formatValue(data.low)} – ${formatValue(data.high, section.unit)}`}</p>}
    {data.empty ? <p className="panel-empty">{donut ? 'No positive values to plot.' : 'No chart data yet.'}</p> : <>
      <svg viewBox="0 0 500 220" role="img" aria-label={`${section.title} — ${data.variant.replace('_', ' ')}`}>
        {donut ? data.slices.map(slice => <circle key={slice.index} cx="250" cy="105" r="72"
          fill="none" stroke={seriesColor(slice.index)} strokeWidth="28" pathLength="100"
          strokeDasharray={`${slice.fraction * 100} ${100 - slice.fraction * 100}`}
          strokeDashoffset={-slice.offset * 100} transform="rotate(-90 250 105)" />) : <>
          <line x1="48" x2="488" y1={data.zero} y2={data.zero} className="panel-chart-axis" />
          {data.variant === 'area' && data.areas.map(area => <polygon key={area.index}
            points={area.points} fill={seriesColor(area.index)} fillOpacity=".15" />)}
          {(data.variant === 'line' || data.variant === 'area') && data.lines.map(line => <polyline key={line.index}
            points={line.points} fill="none" stroke={seriesColor(line.index)} strokeWidth="3" />)}
          {(data.variant === 'line' || data.variant === 'area') && data.dots.map(dot => <circle key={dot.key}
            cx={dot.x} cy={dot.y} r="3" fill={seriesColor(dot.index)}>
            <title>{dot.label}: {exactValue(dot.value, section.unit)}</title>
          </circle>)}
          {(data.variant === 'bar' || data.variant === 'stacked_bar') && data.bars.map(bar => <rect key={bar.key}
            x={bar.x} y={bar.y} width={bar.width} height={bar.height} fill={seriesColor(bar.index)} rx="2" />)}
          {data.ticks.map((tick, i) => <text key={i} x={tick.x} y="208" textAnchor={i === 0 ? 'start' : tick.x > 450 ? 'end' : 'middle'} className="panel-chart-label">
            <title>{tick.label}</title>{tick.label.length > 7 ? `${tick.label.slice(0, 6)}…` : tick.label}
          </text>)}
        </>}
      </svg>
      <ul className="panel-chart-legend" aria-label="Chart legend">
        {(donut ? data.slices.map(s => ({ name: `${s.label}: ${formatValue(s.value, section.unit)}`, index: s.index }))
          : data.series.map((s, index) => ({ name: s.name, index }))).map(item => <li key={item.index}>
          <span aria-hidden="true" style={{ background: seriesColor(item.index) }} />{item.name}
        </li>)}
      </ul>
    </>}
    <details>
      <summary>Chart values</summary>
      <div className="panel-table-scroll"><table>
        <caption>{section.title}{section.unit ? ` (${section.unit})` : ''}</caption>
        <thead><tr><th scope="col">Label</th>{data.series.map((s, i) => <th scope="col" key={i}>{s.name}</th>)}</tr></thead>
        <tbody>{data.labels.map((label, i) => <tr key={i}><th scope="row">{label}</th>
          {data.series.map((s, j) => <td key={j}>{exactValue(s.points[i].value, section.unit)}</td>)}
        </tr>)}</tbody>
      </table></div>
    </details>
  </>;
}
