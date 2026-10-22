import type { ReactNode } from 'react';
import { identityRequest, workspaceId } from '../workspace-context';
import { Button, Tabs } from './ui';
import { useMutation, useResource } from './requests';

type AgentGrant = { user: string; agent: string };
type ResourceGrant = { user: string; kind: string; resource: string; access: string };
type Resource = { id: string; name?: string; active?: { definition: { title: string } } | null };
type Directory = { role: string; agents: { id: string; name: string }[]; grants: AgentGrant[] };
type Resources = { apps: Resource[]; panels: Resource[]; grants: ResourceGrant[] };
const base = `/workspaces/${workspaceId}`;
async function loadAccess() {
  const [directory, resources] = await Promise.all([
    identityRequest<Directory>(`${base}/chat-directory`), identityRequest<Resources>(`${base}/resource-access`),
  ]);
  if (directory.role !== 'Owner') throw new Error('Only Owners can manage resource access.');
  return { directory, resources };
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
  const resources = data ? <>
    <p className="ui-help">Share Apps and Home panels with this person. Record access does not grant app configuration or release permissions. Changes save immediately.</p>
    {([['app', data.resources.apps], ['panel', data.resources.panels.filter(p => p.active)]] as const).map(([kind, items]) => <section key={kind}>
      {!!items.length && <h3>{kind === 'app' ? 'Apps' : 'Home panels'}</h3>}
      {items.map(resource => <label className="ui-resource" key={resource.id}>{resource.name ?? resource.active?.definition.title ?? 'Home panel'}
        <select className="ui-select" value={data.resources.grants.find(g => g.user === user && g.kind === kind && g.resource === resource.id)?.access ?? ''} onChange={event => {
          const access = event.target.value;
          void mutation.run(async () => {
            await identityRequest(`${base}/resource-access`, { user, kind, resource: resource.id, access: access || null });
            setData(current => current && ({ ...current, resources: { ...current.resources, grants: [
              ...current.resources.grants.filter(g => !(g.user === user && g.kind === kind && g.resource === resource.id)),
              ...(access ? [{ user, kind, resource: resource.id, access }] : []),
            ] } })); mutation.setStatus('Resource access saved.');
          });
        }}><option value="">No access</option><option value="read">Read only</option>{kind === 'app' && <option value="write">Read, edit and delete records</option>}</select>
      </label>)}
    </section>)}
    {!data.resources.apps.length && !data.resources.panels.some(p => p.active) && <p className="ui-empty">Publish an App or keep a panel on Home to share it here.</p>}
  </> : loading;
  return <Tabs sections={[{ title: 'Membership', content: membership }, { title: 'Agents', content: agents }, { title: 'Apps & panels', content: resources }]} />;
}
