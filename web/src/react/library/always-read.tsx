import { useCallback, useRef, useState } from 'react';
import { identityRequest, workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { useForName } from './model';
import './always-read.css';

type Given = {
  id: string;
  title: string;
  folder?: string | null;
  use_for: string;
  characters: number;
};
type Listing = {
  items: Given[];
  given: Given[];
  hidden: number;
  chosen: number;
  editable: boolean;
  most: number;
  budget: number;
};

/** Documents an agent always has in every request (Library LB2c). Its managers choose
 * them from the documents it was given; each request still follows "use for". */
export function AlwaysRead({ agent }: { agent: { id: string; name: string } }) {
  const path = `/workspaces/${workspaceId}/library-always/${encodeURIComponent(agent.id)}`;
  const load = useCallback(() => identityRequest<Listing>(path), [path]);
  const listing = useResource(load);
  const mutation = useMutation();
  const [draft, setDraft] = useState<string[]>();
  const [adding, setAdding] = useState('');
  const saving = useRef(false);
  const data = listing.data;
  if (!data)
    return listing.error ? (
      <p className="ui-status" role="alert">
        {listing.error}
      </p>
    ) : null;
  const saved = data.items.map((i) => i.id);
  const chosen = draft ?? saved;
  const byId = (id: string) => data.given.find((g) => g.id === id);
  const used = chosen.reduce((sum, id) => sum + (byId(id)?.characters ?? 0), 0);
  const changed = draft !== undefined && JSON.stringify(draft) !== JSON.stringify(saved);
  const save = () => {
    if (saving.current || !draft) return;
    saving.current = true;
    void mutation
      .run(async () => {
        await identityRequest(path, { items: draft, expected: saved });
        await listing.refresh();
        setDraft(undefined);
        mutation.setStatus('Saved.');
      })
      .finally(() => {
        saving.current = false;
      });
  };
  return (
    <section className="library-always" aria-label={`Documents ${agent.name} always reads`}>
      <h4>Always reads</h4>
      <p className="ui-help">
        Up to {data.most} Library documents {agent.name} has in every request, about{' '}
        {data.budget.toLocaleString()} characters together. Each request still follows who the
        documents may answer.
      </p>
      {chosen.length === 0 && <p className="ui-help">None yet.</p>}
      <ol>
        {chosen.map((id, index) => {
          const document = byId(id);
          return (
            <li key={id}>
              <span>
                {document?.title ?? 'A document'}
                <small>
                  {document?.folder ? `${document.folder} · ` : ''}
                  {useForName(document?.use_for ?? '')}
                </small>
              </span>
              {data.editable && (
                <span className="library-always-actions">
                  <Button
                    aria-label={`Move ${document?.title} up`}
                    disabled={index === 0 || mutation.busy}
                    onClick={() => {
                      const next = [...chosen];
                      [next[index - 1], next[index]] = [next[index], next[index - 1]];
                      setDraft(next);
                    }}
                  >
                    ↑
                  </Button>
                  <Button
                    aria-label={`Remove ${document?.title}`}
                    disabled={mutation.busy}
                    onClick={() => setDraft(chosen.filter((c) => c !== id))}
                  >
                    Remove
                  </Button>
                </span>
              )}
            </li>
          );
        })}
      </ol>
      {data.hidden > 0 && <p className="ui-help">{data.hidden} more you can't open.</p>}
      {data.editable && (
        <div className="library-always-add">
          {data.given.length === 0 ? (
            <p className="ui-help">
              Give {agent.name} documents first: in the Library, Share → Agent access.
            </p>
          ) : (
            <>
              <select
                aria-label="Add a document"
                value={adding}
                disabled={chosen.length >= data.most || mutation.busy}
                onChange={(event) => setAdding(event.target.value)}
              >
                <option value="">Add a document…</option>
                {data.given
                  .filter((g) => !chosen.includes(g.id))
                  .map((g) => (
                    <option key={g.id} value={g.id}>
                      {g.folder ? `${g.folder} › ` : ''}
                      {g.title}
                    </option>
                  ))}
              </select>
              <Button
                disabled={!adding || chosen.length >= data.most || mutation.busy}
                onClick={() => {
                  setDraft([...chosen, adding]);
                  setAdding('');
                }}
              >
                Add
              </Button>
            </>
          )}
          <p className={used > data.budget ? 'ui-status library-over' : 'ui-help'}>
            {used.toLocaleString()} of {data.budget.toLocaleString()} characters
            {used > data.budget ? ' · the end of the list is cut short' : ''}
          </p>
          <div className="agent-save-row">
            <Button primary disabled={!changed || mutation.busy} onClick={save}>
              Save documents
            </Button>
            {changed && (
              <Button disabled={mutation.busy} onClick={() => setDraft(undefined)}>
                Discard
              </Button>
            )}
            <p className="ui-status" role="status">
              {mutation.busy ? 'Saving…' : mutation.status}
            </p>
          </div>
        </div>
      )}
    </section>
  );
}
