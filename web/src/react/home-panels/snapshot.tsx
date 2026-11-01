import type { ReactNode } from 'react';
import type { PanelDefinition } from '../../panel-types';
import { sourceUrl, type Snapshot } from './model';
import { PanelSection } from './sections';
export function PanelContent({
  definition,
  scrollable = false,
  after,
}: {
  definition: PanelDefinition;
  scrollable?: boolean;
  /** Shown after the sections, inside the scrolling area (the frame's note). */
  after?: ReactNode;
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
          <PanelSection section={s} />
        </section>
      ))}
      {after}
    </div>
  );
}
export function SnapshotDetails({ snapshot }: { snapshot: Snapshot }) {
  if (!snapshot.available) return <p>Saved content is hidden because its source access changed.</p>;
  return (
    <div className="workspace-panel-basis">
      <p>Saved {new Date(snapshot.saved_ms!).toLocaleString()} · Personal</p>
      {snapshot.checked_ms && <p>Sources read {new Date(snapshot.checked_ms).toLocaleString()}.
        This records read completion, not the upstream data’s coverage or accuracy.</p>}
      <p>
        {snapshot.source_operations
          ? `${snapshot.source_operations} observed operation${snapshot.source_operations === 1 ? '' : 's'}. Receipts do not prove the interpretation.`
          : 'Agent findings without operation receipts. Review the basis before relying on them.'}
      </p>
      <a href={sourceUrl(snapshot)}>Open source conversation ↗</a>
    </div>
  );
}
