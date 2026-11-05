import { useCallback, useState } from 'react';
import { identityRequest, workspaceId, workspaceUrl } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { appsRequest, builtPath, type BuiltApp } from './apps-model';

type Watchers = {
  collections: string[];
  watchers: {
    collection: string;
    agent: string;
    agent_name: string;
    instructions: string;
    by: string;
    chat: string | null;
    waiting: number;
  }[];
  can_change: boolean;
};

/** App events (AP10): when a record is added to a collection, an agent the builder or
 * an admin chose acts on it, as them, in their conversation with it. */
export function AppEvents({ app }: { app: BuiltApp }) {
  const load = useCallback(() => appsRequest<Watchers>(builtPath(app.id, '/watchers')), [app.id]);
  const watchers = useResource(load);
  const mutation = useMutation();
  const [editing, setEditing] = useState<string>();
  const data = watchers.data;
  if (!data?.collections?.length || (!data.can_change && !data.watchers.length)) return null;
  return (
    <section aria-label="When something is added">
      <h3>When something is added</h3>
      <ul className="app-actions">
        {data.collections.map((collection) => {
          const watcher = data.watchers.find((x) => x.collection === collection);
          return (
            <li key={collection}>
              <span>
                New {collection.replaceAll('_', ' ')}
                <small>
                  {watcher
                    ? `${watcher.agent_name}, for ${watcher.by}: ${watcher.instructions}${watcher.waiting ? ` · ${watcher.waiting} waiting` : ''}`
                    : 'No agent acts on these.'}
                  {watcher?.chat && (
                    <>
                      {' '}
                      <a href={workspaceUrl(`/?view=chat&conversation=${watcher.chat}`)}>
                        See the work
                      </a>
                    </>
                  )}
                </small>
              </span>
              {data.can_change && (
                <span className="apps-row">
                  <Button onClick={() => setEditing(collection)}>
                    {watcher ? 'Change' : 'Ask an agent'}
                  </Button>
                  {watcher && (
                    <Button
                      disabled={mutation.busy}
                      onClick={() =>
                        void mutation.run(async () => {
                          await appsRequest(builtPath(app.id, '/watchers'), {
                            collection,
                            agent: null,
                            instructions: '',
                          });
                          await watchers.refresh();
                        })
                      }
                    >
                      Stop
                    </Button>
                  )}
                </span>
              )}
            </li>
          );
        })}
      </ul>
      {(mutation.status || watchers.error) && (
        <p role="alert">{mutation.status || watchers.error}</p>
      )}
      {editing && (
        <WatchDialog
          app={app}
          collection={editing}
          current={data.watchers.find((x) => x.collection === editing)}
          close={() => setEditing(undefined)}
          saved={() => void watchers.refresh().catch(() => {})}
        />
      )}
    </section>
  );
}

function WatchDialog({
  app,
  collection,
  current,
  close,
  saved,
}: {
  app: BuiltApp;
  collection: string;
  current?: Watchers['watchers'][number];
  close: () => void;
  saved: () => void;
}) {
  const loadAgents = useCallback(
    () =>
      identityRequest<{ agents: { id: string; name: string }[] }>(
        `/workspaces/${workspaceId}/chat-directory`,
      ),
    [],
  );
  const agents = useResource(loadAgents);
  const mutation = useMutation();
  const [agent, setAgent] = useState(current?.agent ?? '');
  const [instructions, setInstructions] = useState(current?.instructions ?? '');
  const label = collection.replaceAll('_', ' ');
  return (
    <Dialog
      title={`When new ${label} are added`}
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
                await appsRequest(builtPath(app.id, '/watchers'), {
                  collection,
                  agent,
                  instructions,
                });
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
        The agent acts on each new one as you, in a conversation with you, and can do in {app.name}{' '}
        only what you could and what Agents can allows.
      </p>
      <label className="ui-field">
        Agent
        <select aria-label="Agent" value={agent} onChange={(e) => setAgent(e.target.value)}>
          <option value="">Choose an agent</option>
          {agents.data?.agents.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </select>
      </label>
      <label className="ui-field">
        What should it do?
        <textarea
          aria-label="What should it do?"
          rows={4}
          maxLength={2000}
          placeholder="Tell me who asked and for how long, and open a task for me to decide."
          value={instructions}
          onChange={(e) => setInstructions(e.target.value)}
        />
      </label>
    </Dialog>
  );
}
