import { useState } from 'react';
import type { Comparison, Section, TableSection } from '../../panel-types';
import { comparisonView, formatValue, progressView, sortedRows } from '../../panel-data';
import { PanelChart } from './chart';
import '../../panel-visualizations.css';

function MetricComparison({ value }: { value?: Comparison }) {
  if (!value) return null;
  const comparison = comparisonView(value);
  return <p className="panel-comparison" data-sentiment={comparison.sentiment}>{comparison.text}</p>;
}
function PanelTable({ section }: { section: TableSection }) {
  const [sort, setSort] = useState<{ column: number; descending: boolean }>();
  return <>
    {!section.rows.length && <p className="panel-empty">No records yet.</p>}
    <div className="panel-table-scroll"><table>
      <thead><tr>{section.columns.map((column, i) => <th key={i} scope="col"
        aria-sort={sort?.column === i ? sort.descending ? 'descending' : 'ascending' : 'none'}>
        <button type="button" onClick={() => setSort({ column: i, descending: sort?.column === i && !sort.descending })}>
          {column}{sort?.column === i ? sort.descending ? ' ↓' : ' ↑' : ''}
        </button>
      </th>)}</tr></thead>
      <tbody>{sortedRows(section, sort).map(row => <tr key={row.index}>
        {row.values.map((cell, i) => <td key={i}>{cell}</td>)}
      </tr>)}</tbody>
    </table></div>
  </>;
}
export function PanelSection({ section: s }: { section: Section }) {
  switch (s.kind) {
    case 'text': return <><h4>{s.heading}</h4><p className="panel-text">{s.body}</p></>;
    case 'metric': return <><p>{s.label}</p><strong className="panel-metric-value">{s.value}</strong><MetricComparison value={s.comparison} /></>;
    case 'metrics': return <div className="panel-metric-group">{s.items.map(item => <div key={item.id}>
      <p>{item.label}</p><strong className="panel-metric-value">{item.value}</strong><MetricComparison value={item.comparison} />
    </div>)}</div>;
    case 'table': return <PanelTable section={s} />;
    case 'chart': return <PanelChart section={s} />;
    case 'collection': return s.items.length ? <ul className="panel-collection">{s.items.map(item => <li key={item.id}>
      <div><strong>{item.title}</strong>{item.description && <p>{item.description}</p>}</div>
      {item.status && <span className="panel-item-status">{item.status}</span>}
    </li>)}</ul> : <p className="panel-empty">No items yet.</p>;
    case 'progress': return <div className="panel-progress-group">{s.items.map(item => {
      const view = progressView(item);
      const description = `${formatValue(item.actual, item.unit)} actual · ${formatValue(item.target, item.unit)} target`;
      return <div key={item.id}><p>{item.label}</p><strong className="panel-progress-value">{view.percentage}</strong>
        <div className="panel-progress-track" role="progressbar" aria-label={item.label}
          aria-valuemin={0} aria-valuemax={100} aria-valuenow={view.width} aria-valuetext={description}>
          <span style={{ width: `${view.width}%` }} />
        </div><p className="panel-progress-description">{description}</p>
        {Boolean(item.minimum) && <small>Range starts at {formatValue(item.minimum!, item.unit)}</small>}
      </div>;
    })}</div>;
    case 'pipeline': {
      const top = Math.max(1, ...s.stages.map(stage => stage.count));
      return <ol className="panel-pipeline">{s.stages.map(stage => <li key={stage.id}>
        <span className="panel-pipeline-label">{stage.label}</span>
        <span className="panel-pipeline-bar" aria-hidden="true"><span style={{ width: `${Math.round(stage.count / top * 100)}%` }} /></span>
        <strong>{formatValue(stage.count)}</strong>{stage.value && <small>{stage.value}</small>}
      </li>)}</ol>;
    }
    case 'timeline': return s.events.length ? <ol className="panel-timeline">{[...s.events].sort((a, b) => b.at_ms - a.at_ms).map(event => <li key={event.id}>
      <time dateTime={new Date(event.at_ms).toISOString()}>{new Date(event.at_ms).toLocaleString()}</time>
      <div><strong>{event.label}</strong>{event.description && <p>{event.description}</p>}</div>
    </li>)}</ol> : <p className="panel-empty">No events yet.</p>;
  }
}
