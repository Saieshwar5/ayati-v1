import { useCallback, useRef, useState } from 'react';
import {
  identityRequest,
  workspaceId,
  workspaceSession,
  workspaceUrl,
} from '../../workspace-context';
import { navigate } from '../../navigation';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import {
  request,
  itemUrl,
  dateLabel,
  levelName,
  useForName,
  type AgentGrant,
  type Item,
} from './model';
type Directory = {
  users: { id: string; name: string }[];
  agents: { id: string; name: string; audience?: string; can_talk?: boolean; settings?: { paused: boolean } }[];
};
async function loadDirectory(): Promise<Directory> {
  const [people, agents] = await Promise.all([
    identityRequest<{ users: Directory['users'] }>(`/workspaces/${workspaceId}/chat-directory`),
    identityRequest<{ agents: Directory['agents'] }>(`/workspaces/${workspaceId}/agent-profiles`),
  ]);
  return {
    users: people.users,
    agents: agents.agents.filter((a) => a.audience !== 'only_me' && a.can_talk !== false && !a.settings?.paused),
  };
}
export function useAttempt() {
  const attempt = useRef<{ payload: string; request: string }>(undefined);
  return (payload: unknown) => {
    const key = JSON.stringify(payload);
    if (attempt.current?.payload !== key)
      attempt.current = { payload: key, request: crypto.randomUUID() };
    return { ...(payload as Record<string, unknown>), request: attempt.current.request };
  };
}
export function ShareDialog({
  item,
  close,
  changed,
}: {
  item: Item;
  close: () => void;
  changed: () => void;
}) {
  const directory = useResource(loadDirectory),
    mutation = useMutation(),
    withRequest = useAttempt();
  const [visibility, setVisibility] = useState(item.visibility);
  const [people, setPeople] = useState<Record<string, string>>(
    Object.fromEntries(item.people.map((p) => [p.user, p.access])),
  );
  // Agent id → what it may do and whose questions it may answer from this item.
  const [agents, setAgents] = useState<Record<string, Omit<AgentGrant, 'agent'>>>(
    Object.fromEntries(item.agents.map((g) => [g.agent, { level: g.level, use_for: g.use_for }])),
  );
  const folder = item.kind === 'folder';
  // Owner decision: agents suggest edits by default; files can only be read.
  const levels: AgentGrant['level'][] =
    item.kind === 'file' ? ['read'] : folder ? ['read', 'suggest', 'edit', 'save'] : ['read', 'suggest', 'edit'];
  return (
    <Dialog
      title="People and agents"
      drawer
      close={close}
      busy={mutation.busy}
      status={mutation.status || directory.error}
      footer={
        <>
          <Button disabled={mutation.busy} onClick={close}>
            Cancel
          </Button>
          <Button
            primary
            disabled={!directory.data || mutation.busy}
            onClick={() =>
              void mutation.run(async () => {
                await request(
                  `/${item.id}/share`,
                  withRequest({
                    revision: item.access_revision,
                    visibility,
                    people:
                      visibility === 'private'
                        ? []
                        : Object.entries(people).map(([user, access]) => ({
                            user,
                            access,
                          })),
                    agents: Object.entries(agents).map(([agent, grant]) => ({ agent, ...grant })),
                  }),
                );
                changed();
                close();
              })
            }
          >
            Save changes
          </Button>
        </>
      }
    >
      <p>
        {folder
          ? 'Everything in this folder follows these settings.'
          : 'Choose who can open this item.'}{' '}
        Sharing does not invite anyone into the workspace.
      </p>
      <label className="library-field">
        People access
        <select
          aria-label="People access"
          value={visibility}
          onChange={(event) => setVisibility(event.target.value as Item['visibility'])}
        >
          <option value="private">Only me</option>
          <option value="selected">Selected people</option>
          <option value="workspace">Workspace · everyone can view</option>
        </select>
      </label>
      {visibility !== 'private' && (
        <div className="library-grants">
          {directory.data?.users
            .filter((p) => p.id !== workspaceSession?.user?.id)
            .map((person) => (
              <label key={person.id}>
                <span>{person.name}</span>
                <select
                  aria-label={`Access for ${person.name}`}
                  value={people[person.id] ?? ''}
                  onChange={(event) =>
                    setPeople((old) => {
                      const next = { ...old };
                      if (event.target.value) next[person.id] = event.target.value;
                      else delete next[person.id];
                      return next;
                    })
                  }
                >
                  <option value="">
                    {visibility === 'workspace' ? 'Workspace view' : 'No access'}
                  </option>
                  <option value="view">Can view</option>
                  {item.kind !== 'file' && <option value="edit">Can edit</option>}
                </select>
              </label>
            ))}
        </div>
      )}
      <section className="library-agent-grants">
        <h3>Agent access</h3>
        <p>
          Selected agents may read {folder ? 'what is in this folder' : 'this item'} to answer
          questions. Suggested edits wait for someone who can edit; edits are saved as versions
          you can restore. Agents never change who has access.
        </p>
        {directory.data?.agents.map((agent) => (
          <div className="library-agent-grant" key={agent.id}>
            <label className="library-checkbox">
              <input
                type="checkbox"
                checked={agent.id in agents}
                onChange={(event) =>
                  setAgents((old) => {
                    const next = { ...old };
                    if (event.target.checked)
                      next[agent.id] = {
                        level: item.kind === 'file' ? 'read' : 'suggest',
                        use_for: 'viewers',
                      };
                    else delete next[agent.id];
                    return next;
                  })
                }
              />
              {agent.name}
              <small>{agent.id in agents ? levelName(agents[agent.id].level) : 'No access'}</small>
            </label>
            {agent.id in agents && levels.length > 1 && (
              <label className="library-use-for">
                Can
                <select
                  aria-label={`What ${agent.name} can do`}
                  value={agents[agent.id].level}
                  onChange={(event) =>
                    setAgents((old) => ({
                      ...old,
                      [agent.id]: {
                        ...old[agent.id],
                        level: event.target.value as AgentGrant['level'],
                      },
                    }))
                  }
                >
                  {levels.map((level) => (
                    <option key={level} value={level}>
                      {levelName(level)}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {agent.id in agents && (
              <label className="library-use-for">
                Use for
                <select
                  aria-label={`Use for, ${agent.name}`}
                  value={agents[agent.id].use_for}
                  onChange={(event) =>
                    setAgents((old) => ({
                      ...old,
                      [agent.id]: {
                        ...old[agent.id],
                        use_for: event.target.value as AgentGrant['use_for'],
                      },
                    }))
                  }
                >
                  <option value="viewers">{useForName('viewers')}</option>
                  <option value="staff">{useForName('staff')}</option>
                </select>
              </label>
            )}
          </div>
        ))}
        {directory.data?.agents.length === 0 && (
          <p>No available workspace agents. Create or request access to one in Agents.</p>
        )}
        <small>
          People who can see it: the agent uses it only when everyone who will read the answer can
          open it. All staff: any colleague's question can use it, without opening the document.
        </small>
      </section>
      <section className="library-link">
        <h3>Link to this item</h3>
        <p>A link does not change permissions.</p>
        <Button
          onClick={() =>
            void mutation.run(async () => {
              await navigator.clipboard.writeText(
                new URL(itemUrl(item.id), window.location.href).href,
              );
              mutation.setStatus('Link copied.');
            })
          }
        >
          Copy link
        </Button>
      </section>
    </Dialog>
  );
}
export function AskDialog({ item, close }: { item: Item; close: () => void }) {
  const directory = useResource(loadDirectory),
    mutation = useMutation(),
    withRequest = useAttempt();
  const [agent, setAgent] = useState(''),
    [question, setQuestion] = useState('');
  const eligible =
    directory.data?.agents.filter((a) => item.agents.some((g) => g.agent === a.id)) ?? [];
  return (
    <Dialog
      title="Ask an agent"
      close={close}
      busy={mutation.busy}
      status={mutation.status || directory.error}
      footer={
        <Button
          primary
          disabled={mutation.busy || !agent || !question.trim()}
          onClick={() =>
            void mutation.run(async () => {
              const result = await request<{ chat: string }>(
                `/${item.id}/ask`,
                withRequest({ revision: item.revision, agent, question }),
              );
              navigate(workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(result.chat)}`));
              close();
            })
          }
        >
          Continue in Chat
        </Button>
      }
    >
      <p>
        Use the saved version of <strong>{item.title}</strong> (revision {item.revision}) in a new
        private conversation.
      </p>
      <label className="library-field">
        Agent
        <select aria-label="Agent" value={agent} onChange={(event) => setAgent(event.target.value)}>
          <option value="">Choose an agent</option>
          {eligible.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </select>
      </label>
      {directory.data && eligible.length === 0 && (
        <p>
          You need an available workspace agent that the creator has allowed to read this item. Ask
          the creator or a workspace admin if none are available.
        </p>
      )}
      <label className="library-field">
        Your question
        <textarea
          aria-label="Your question"
          autoFocus
          value={question}
          maxLength={4000}
          rows={5}
          placeholder="What would you like to understand or prepare?"
          onChange={(event) => setQuestion(event.target.value)}
        />
      </label>
      <p className="library-muted">
        The agent receives a bounded source excerpt. Answers need review; reading this source grants
        no permission to act. Your configured model provider processes the request.
      </p>
    </Dialog>
  );
}
export function HistoryDialog({
  item,
  close,
  changed,
}: {
  item: Item;
  close: () => void;
  changed: () => void;
}) {
  const load = useCallback(
    () =>
      request<{
        versions: {
          revision: number;
          title: string;
          author: string;
          agent?: string | null;
          created: number;
        }[];
      }>(`/${item.id}/versions`),
    [item.id],
  );
  const history = useResource(load),
    mutation = useMutation(),
    withRequest = useAttempt();
  const [preview, setPreview] = useState<{
    title: string;
    revision: number;
    document: unknown;
  }>();
  return (
    <Dialog
      title="Version history"
      drawer
      close={close}
      busy={mutation.busy}
      status={mutation.status || history.error}
    >
      <p>
        Inspect the last 50 saved revisions. Restoring creates a new revision. Versions referenced
        in Chat are also retained.
      </p>
      {preview && (
        <section className="library-version-preview">
          <h3>
            Revision {preview.revision} · {preview.title}
          </h3>
          <pre>{pageText(preview.document)}</pre>
        </section>
      )}
      {history.data?.versions.map((version) => (
        <div className="library-version" key={version.revision}>
          <div>
            <strong>
              Revision {version.revision} · {version.title}
            </strong>
            <small>
              {version.author}
              {version.agent ? ` · by ${version.agent}` : ''} · {dateLabel(version.created)}
            </small>
          </div>
          <Button
            disabled={mutation.busy}
            onClick={() =>
              void mutation.run(async () =>
                setPreview(await request(`/${item.id}/versions/${version.revision}`)),
              )
            }
          >
            View
          </Button>
          <Button
            disabled={mutation.busy || version.revision === item.revision}
            onClick={() =>
              void mutation.run(async () => {
                await request(
                  `/${item.id}/action`,
                  withRequest({
                    revision: item.revision,
                    action: 'restore_version',
                    version: version.revision,
                  }),
                );
                changed();
                close();
              })
            }
          >
            Restore
          </Button>
        </div>
      ))}
    </Dialog>
  );
}

function pageText(value: unknown): string {
  if (!value || typeof value !== 'object') return '';
  const node = value as { type?: string; text?: string; content?: unknown[] };
  return (
    (node.text ?? '') +
    (node.content?.map(pageText).join('') ?? '') +
    (['paragraph', 'heading', 'codeBlock', 'hardBreak'].includes(node.type ?? '') ? '\n' : '')
  );
}
