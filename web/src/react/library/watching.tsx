import { useCallback, useState } from 'react';
import { identityRequest, workspaceId, workspaceUrl } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { dateLabel, request, type Item } from './model';

type Watcher = {
  watcher: {
    agent: string;
    agent_name: string;
    instructions: string;
    chat: string | null;
    by: string;
    waiting: number;
  } | null;
  can_change: boolean;
};

/** A watched folder (LB6): an agent handles each new item, in its own conversation
 * with the person who set it up. Its manager chooses the agent and what it does. */
export function FolderWatch({ item }: { item: Item }) {
  const load = useCallback(() => request<Watcher>(`/${item.id}/watch`), [item.id]);
  const watch = useResource(load);
  const mutation = useMutation();
  const [setting, setSetting] = useState(false);
  const data = watch.data;
  if (!data || (!data.watcher && !data.can_change)) return null;
  return (
    <div className="library-watch">
      {data.watcher ? (
        <p className="library-muted">
          {data.watcher.agent_name} handles new items here for {data.watcher.by}:{' '}
          {data.watcher.instructions}
          {data.watcher.waiting > 0 && ` · ${data.watcher.waiting} waiting`}
          {data.watcher.chat && (
            <>
              {' '}
              <a href={workspaceUrl(`/?view=chat&conversation=${data.watcher.chat}`)}>
                See the work
              </a>
            </>
          )}
        </p>
      ) : null}
      {data.can_change && (
        <div className="apps-row">
          <Button onClick={() => setSetting(true)}>
            {data.watcher ? 'Change watching' : 'Watch with an agent'}
          </Button>
          {data.watcher && (
            <Button
              disabled={mutation.busy}
              onClick={() =>
                void mutation.run(async () => {
                  await request(`/${item.id}/watch`, { agent: null, instructions: '' });
                  await watch.refresh();
                })
              }
            >
              Stop watching
            </Button>
          )}
        </div>
      )}
      {(mutation.status || watch.error) && <p role="alert">{mutation.status || watch.error}</p>}
      {setting && (
        <WatchDialog
          item={item}
          current={data.watcher}
          close={() => setSetting(false)}
          saved={() => void watch.refresh().catch(() => {})}
        />
      )}
    </div>
  );
}

type Agent = { id: string; name: string };
function WatchDialog({
  item,
  current,
  close,
  saved,
}: {
  item: Item;
  current: Watcher['watcher'];
  close: () => void;
  saved: () => void;
}) {
  const loadAgents = useCallback(
    () => identityRequest<{ agents: Agent[] }>(`/workspaces/${workspaceId}/chat-directory`),
    [],
  );
  const agents = useResource(loadAgents);
  const mutation = useMutation();
  const given = agents.data?.agents.filter((a) => item.agents.some((g) => g.agent === a.id)) ?? [];
  const [agent, setAgent] = useState(current?.agent ?? '');
  const [instructions, setInstructions] = useState(current?.instructions ?? '');
  return (
    <Dialog
      title={`Watch ${item.title} with an agent`}
      close={close}
      busy={mutation.busy}
      status={mutation.status || agents.error}
      footer={
        <>
          <Button disabled={mutation.busy} onClick={close}>
            Cancel
          </Button>
          <Button
            primary
            disabled={mutation.busy || !agent || !instructions.trim()}
            onClick={() =>
              void mutation.run(async () => {
                await request(`/${item.id}/watch`, { agent, instructions });
                saved();
                close();
              })
            }
          >
            Save
          </Button>
        </>
      }
    >
      <p>
        Each new item in this folder becomes work for the agent, in a conversation with you. It
        reaches only what you and its Library access allow.
      </p>
      <label className="ui-field">
        Agent
        <select aria-label="Agent" value={agent} onChange={(e) => setAgent(e.target.value)}>
          <option value="">Choose an agent given this folder</option>
          {given.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </select>
      </label>
      {agents.data && !given.length && (
        <p className="library-muted">Give an agent this folder first, from Share folder.</p>
      )}
      <label className="ui-field">
        What should it do with each new item?
        <textarea
          aria-label="What should it do with each new item?"
          rows={4}
          maxLength={2000}
          placeholder="Record the supplier, amount and due date, file it under Invoices, and ask me whether to pay."
          value={instructions}
          onChange={(e) => setInstructions(e.target.value)}
        />
      </label>
    </Dialog>
  );
}

type Fields = {
  fields: {
    fields: Record<string, string | number | boolean | null>;
    agent: string | null;
    at: number;
  } | null;
};
/** Key fields an agent recorded on an item, such as an invoice's amount and due date. */
export function ItemFields({ item }: { item: Item }) {
  const load = useCallback(() => request<Fields>(`/${item.id}/fields`), [item.id]);
  const fields = useResource(load);
  const recorded = fields.data?.fields;
  if (!recorded || !Object.keys(recorded.fields).length) return null;
  return (
    <section className="library-fields" aria-label="Key fields">
      <dl>
        {Object.entries(recorded.fields).map(([name, value]) => (
          <div key={name}>
            <dt>{name.replaceAll('_', ' ')}</dt>
            <dd>{value === null ? '—' : String(value)}</dd>
          </div>
        ))}
      </dl>
      <p className="library-muted">
        Recorded by {recorded.agent ?? 'an agent'} · {dateLabel(recorded.at)}
      </p>
    </section>
  );
}
