import type { ReactNode } from 'react';
import { identityRequest, workspaceId } from '../workspace-context';
import { Button, Tabs } from './ui';
import { useMutation, useResource } from './requests';

type AgentGrant = { user: string; agent: string };
type Directory = { role: string; agents: { id: string; name: string }[]; grants: AgentGrant[] };
const base = `/workspaces/${workspaceId}`;
async function loadAccess() {
  const directory = await identityRequest<Directory>(`${base}/chat-directory`);
  if (directory.role !== 'Owner') throw new Error('Only Owners can manage agent access.');
  return { directory };
}

export function MemberAccess({ user, mutation, membership }: { user: string; mutation: ReturnType<typeof useMutation>; membership: ReactNode }) {
  const { data, setData, error, refresh } = useResource(loadAccess);
  const loading = <><p className="ui-help">{error || 'Loading access…'}</p>{error && <Button onClick={() => void refresh().catch(() => {})}>Retry</Button>}</>;
  const agents = data ? <>
    <p className="ui-help">Allow this person to ask agents for replies in shared Chat. Private history, configuration and business tools stay separate. Changes save immediately.</p>
    {data.directory.agents.map(agent => <label className="ui-check" key={agent.id}>
      <input type="checkbox" checked={data.directory.grants.some(g => g.user === user && g.agent === agent.id)} onChange={event => {
        const allowed = event.target.checked;
        void mutation.run(async () => {
          await identityRequest(`${base}/chat-grants`, { user, agent: agent.id, allowed });
          setData(current => current && ({ ...current, directory: { ...current.directory, grants: [
            ...current.directory.grants.filter(g => !(g.user === user && g.agent === agent.id)), ...(allowed ? [{ user, agent: agent.id }] : []),
          ] } })); mutation.setStatus('Agent access saved.');
        });
      }} /><span>{agent.name}</span>
    </label>)}{!data.directory.agents.length && <p className="ui-empty">No agents available to share.</p>}
  </> : loading;
  return <Tabs sections={[{ title: 'Membership', content: membership }, { title: 'Agents', content: agents }]} />;
}
