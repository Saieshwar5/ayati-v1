import type { PanelDefinition, Section } from '../../panel-renderer';
import { sourceUrl, type Snapshot } from './model';
function Chart({ section }: { section: Extract<Section, { kind: 'chart' }> }) {
  const values = section.points.map((p) => p.value),
    low = Math.min(...values),
    high = Math.max(...values);
  const points = section.points
    .map(
      (p, i) =>
        `${20 + (i * 460) / Math.max(1, section.points.length - 1)},${160 - ((p.value / 2 - low / 2) / (high / 2 - low / 2 || 1)) * 140}`,
    )
    .join(' ');
  return (
    <>
      <h4>{section.title}</h4>
      <svg viewBox="0 0 500 180" role="img" aria-label={section.title}>
        <polyline points={points} fill="none" stroke="currentColor" strokeWidth="3" />
      </svg>
      <details>
        <summary>Chart values</summary>
        {section.points.map((p, i) => (
          <p key={i}>
            {p.label}: {p.value}
          </p>
        ))}
      </details>
    </>
  );
}
export function PanelContent({
  definition,
  scrollable = false,
}: {
  definition: PanelDefinition;
  scrollable?: boolean;
}) {
  return (
    <div
      className="panel-snapshot"
      tabIndex={scrollable ? 0 : undefined}
      role={scrollable ? 'region' : undefined}
      aria-label={scrollable ? `${definition.title} saved content` : undefined}
    >
      <h3>{definition.title}</h3>
      <p className="panel-purpose">{definition.purpose}</p>
      {definition.sections.map((s, i) => (
        <section className={`panel-block panel-${s.kind}`} key={i}>
          {s.kind === 'text' ? (
            <>
              <h4>{s.heading}</h4>
              <p className="panel-text">{s.body}</p>
            </>
          ) : s.kind === 'metric' ? (
            <>
              <p>{s.label}</p>
              <strong className="panel-metric-value">{s.value}</strong>
            </>
          ) : s.kind === 'table' ? (
            <div className="panel-table-scroll">
              <table>
                <thead>
                  <tr>
                    {s.columns.map((c, j) => (
                      <th key={j}>{c}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {s.rows.map((r, j) => (
                    <tr key={j}>
                      {r.map((v, k) => (
                        <td key={k}>{v}</td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <Chart section={s} />
          )}
        </section>
      ))}
    </div>
  );
}
export function SnapshotDetails({ snapshot }: { snapshot: Snapshot }) {
  if (!snapshot.available) return <p>Saved content is hidden because its source access changed.</p>;
  return (
    <div className="workspace-panel-basis">
      <p>Saved {new Date(snapshot.saved_ms!).toLocaleString()} · Personal</p>
      <p>
        {snapshot.source_operations
          ? `${snapshot.source_operations} observed operation${snapshot.source_operations === 1 ? '' : 's'}. Receipts do not prove the interpretation.`
          : 'Agent findings without operation receipts. Review the basis before relying on them.'}
      </p>
      <a href={sourceUrl(snapshot)}>Open source conversation ↗</a>
    </div>
  );
}
