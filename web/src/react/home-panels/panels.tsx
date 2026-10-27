import { useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { usePanels } from './use-panels';
import { saveLayout, type HomeLayout, type ChangeTarget } from './model';
import { PanelContent } from './snapshot';
import { AskPanel } from './create';
import { PanelReview } from './review';
import { PanelMaintenance } from './maintenance';
import '../../home-panels.css';
import './panels.css';
export function WorkspacePanels() {
  const { data, error, busy, refresh } = usePanels();
  const [arrange, setArrange] = useState(false);
  const [hidden, setHidden] = useState(false);
  const [change, setChange] = useState<ChangeTarget>();
  const [ask, setAsk] = useState(false);
  const [details, setDetails] = useState<string>();
  const [review, setReview] = useState<string>();
  const m = useMutation();
  const attempt = useRef<{ key: string; id: string }>(undefined);
  if (data?.available === false) return null;
  const layout = data?.layout;
  const all = data
    ? [...data.panels].sort((a, b) => {
        const rank = (id: string) => {
          const n = data.layout.order.indexOf(id);
          return n < 0 ? 10000 : n;
        };
        return rank(a.id) - rank(b.id);
      })
    : [];
  const panels = all.filter((p) => (hidden ? layout!.hidden.includes(p.id) : !layout!.hidden.includes(p.id)));
  const update = (next: HomeLayout) =>
    void m.run(async () => {
      const key = JSON.stringify(next);
      if (attempt.current?.key !== key) attempt.current = { key, id: crypto.randomUUID() };
      await saveLayout(next, attempt.current.id);
      await refresh();
    });
  const moved = (id: string, offset: number) => {
    if (!layout) return;
    const visibleIndex = panels.findIndex((panel) => panel.id === id);
    const target = panels[visibleIndex + offset];
    if (!target) return;
    const order = all.map((panel) => panel.id);
    const from = order.indexOf(id),
      to = order.indexOf(target.id);
    order.splice(from, 1);
    order.splice(to, 0, id);
    update({ ...layout, order });
  };
  const selected = data?.panels.find((p) => p.id === review);
  const inspected = data?.panels.find((p) => p.id === details);
  return (
    <section className="workspace-panels" aria-label="Your workspace panels">
      <header className="workspace-panels-heading">
        <h2>Your workspace panels</h2>
        <div>
          <Button disabled={!data || m.busy} aria-pressed={arrange} onClick={() => setArrange((v) => !v)}>
            {arrange ? 'Done arranging' : 'Arrange'}
          </Button>
          <Button disabled={!data || m.busy} aria-pressed={hidden} onClick={() => setHidden((v) => !v)}>
            {hidden ? 'Visible panels' : 'Hidden panels'}
          </Button>
          <Button primary onClick={() => setAsk(true)}>
            Ask for a panel
          </Button>
        </div>
      </header>
      <p className="ui-help">Personal views from your agents. Opening Home reads saved results.</p>
      {error && (
        <p role="alert">
          {error}{' '}
          <Button disabled={busy} onClick={() => void refresh()}>
            Retry panels
          </Button>
        </p>
      )}
      {!data && !error && <p role="status">Checking your saved panels…</p>}
      {data && !panels.length && (
        <p className="workspace-panels-empty">
          {hidden
            ? 'No hidden personal panels.'
            : 'Ask an agent to prepare a useful view, then review its draft here.'}
        </p>
      )}
      <div className={`home-panels${arrange ? ' is-arranging' : ''}`}>
        {panels.map((panel) => {
          const snapshot = panel.active ?? panel.draft,
            definition = snapshot?.available ? snapshot.definition : undefined,
            metric = definition?.sections[0]?.kind === 'metric';
          return (
            <article
              className={`home-panel-card${layout?.wide.includes(panel.id) && !metric ? ' panel-wide' : ''}`}
              data-panel={panel.id}
              key={panel.id}
              aria-label={definition?.title ?? 'Panel source unavailable'}
              data-template={definition?.template ?? 'brief'}
              data-maintained={panel.maintenance?.revision ? 'true' : undefined}
            >
              {definition ? <PanelContent definition={definition} scrollable /> : <h3>Panel source unavailable</h3>}
              {definition && snapshot ? (
                <div className="workspace-panel-footer">
                  <small
                    className="workspace-panel-attribution"
                    title={`${panel.agent_name ?? 'Agent'} · Saved ${new Date(snapshot.saved_ms!).toLocaleString()}`}
                  >
                    {panel.agent_name ?? 'Agent'} · Saved {new Date(snapshot.saved_ms!).toLocaleString()} ·
                    Personal
                  </small>
                  <small>
                    {snapshot.source_operations
                      ? `${snapshot.source_operations} source receipt${snapshot.source_operations === 1 ? '' : 's'} · Review the interpretation`
                      : 'Agent findings · Review the basis'}
                  </small>
                  {!panel.active && <small>Draft · Review required</small>}
                  <div>
                    {panel.draft?.available && (
                      <Button onClick={() => setReview(panel.id)}>Review draft {panel.draft.revision}</Button>
                    )}
                    <Button onClick={() => setDetails(panel.id)}>Sources and details</Button>
                    {panel.agent && (
                      <Button
                        onClick={() =>
                          setChange({
                            id: panel.id,
                            revision: panel.revision,
                            agent: panel.agent!,
                            chat: snapshot.chat!,
                          })
                        }
                      >
                        Ask for changes
                      </Button>
                    )}
                    {panel.active && <PanelMaintenance panel={panel} refresh={refresh} />}
                  </div>
                </div>
              ) : (
                <p>Saved content is hidden because its source access changed.</p>
              )}
              {arrange && layout && (
                <div className="workspace-panel-arrange">
                  <Button disabled={m.busy || panels[0]?.id === panel.id} onClick={() => moved(panel.id, -1)}>
                    Move earlier
                  </Button>
                  <Button
                    disabled={m.busy || panels.at(-1)?.id === panel.id}
                    onClick={() => moved(panel.id, 1)}
                  >
                    Move later
                  </Button>
                  {!metric && (
                    <Button
                      disabled={m.busy}
                      onClick={() =>
                        update({
                          ...layout,
                          wide: layout.wide.includes(panel.id)
                            ? layout.wide.filter((id) => id !== panel.id)
                            : [...layout.wide, panel.id],
                        })
                      }
                    >
                      {layout.wide.includes(panel.id) ? 'Standard width' : 'Make wider'}
                    </Button>
                  )}
                  <Button
                    disabled={m.busy}
                    onClick={() =>
                      update({
                        ...layout,
                        hidden: hidden
                          ? layout.hidden.filter((id) => id !== panel.id)
                          : [...layout.hidden, panel.id],
                      })
                    }
                  >
                    {hidden ? 'Show on my Home' : 'Hide from my Home'}
                  </Button>
                </div>
              )}
            </article>
          );
        })}
      </div>
      {m.status && <p role="alert">{m.status}</p>}
      <p className="ui-help">Your arrangement syncs across devices. Hiding changes only your Home.</p>
      {change && (
        <AskPanel
          key={`change.${change.id}.${change.revision}`}
          target={change}
          sourceAvailable={Boolean(
            data?.panels.some((p) => p.id === change.id && (p.active?.available || p.draft?.available)),
          )}
          close={() => setChange(undefined)}
        />
      )}
      {ask && <AskPanel close={() => setAsk(false)} />}{' '}
      {inspected && (
        <PanelReview
          key={`details.${inspected.id}.${inspected.active_revision ?? inspected.revision}`}
          panel={inspected}
          readOnly
          close={() => setDetails(undefined)}
          refresh={refresh}
        />
      )}
      {selected && (
        <PanelReview
          key={`${selected.id}.${selected.revision}`}
          panel={selected}
          close={() => setReview(undefined)}
          refresh={refresh}
        />
      )}
    </section>
  );
}
