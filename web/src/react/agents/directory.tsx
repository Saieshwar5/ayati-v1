import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { useResource } from '../requests';
import { AgentEditor } from './editor';
import { defaults, loadAgents, symbols, base, type Agent } from './model';
import { currentWorkspace, identityRequest, workspaceId, workspaceUrl, workspaceSession } from '../../workspace-context';
import { useMutation } from '../requests';
import { LocalAgents } from './local-agents';
import { AgentControl } from './control';

export function AgentAvatar({ agent }: { agent: Agent }) {
  const avatar = agent.settings.avatar;
  return <span className="ui-avatar agent-profile-avatar">{symbols[avatar] ?? <img alt="" src={`/api/identity${base}/${agent.id}/avatar/${avatar}`} />}</span>;
}
export function AgentsPage() {
  const { data, error, refresh } = useResource(loadAgents);
  const mutation = useMutation();
  const alive = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState('all');
  const [selected, select] = useState<Agent>();
  const [controlled, control] = useState<Agent>();
  useEffect(() => {
    const timer = window.setInterval(() => { if (!document.hidden) void refresh().catch(() => {}); }, 10000);
    return () => clearInterval(timer);
  }, [refresh]);
  const agents = (data?.agents ?? []).map(agent => ({ ...agent, settings: { ...defaults, ...agent.settings } }));
  const filtered = agents.filter(a => (filter === 'all' || a.scope === filter) && `${a.name} ${a.role}`.toLowerCase().includes(query.toLowerCase()));
  return <div id="app"><main className="desk-page" aria-label="Agents">
    <header className="desk-toolbar"><div className="desk-title"><h1>Agents</h1><p>Your personal assistants and workspace teammates.</p></div>
      <input className="ui-search" type="search" aria-label="Find an agent" placeholder="Find an agent" value={query} onChange={e => setQuery(e.target.value)} />
      <Button primary disabled={!data} onClick={() => select({ id: crypto.randomUUID(), name: '', role: '', model: data!.default_model, scope: 'private', revision: 0, settings: { ...defaults }, editable: true })}>＋ Create agent</Button>
    </header>
    <div className="agent-filters" aria-label="Agent visibility">{[['all', 'All agents'], ['private', 'Private'], ['workspace', 'Workspace']].map(([value, label]) => <Button key={value} aria-pressed={filter === value} onClick={() => setFilter(value)}>{label}</Button>)}</div>
    <p className="ui-status" role="status">{error || mutation.status || (!data ? 'Loading agents…' : '')}</p>
    {error && <Button onClick={() => void refresh().catch(() => {})}>Retry</Button>}
    <div className="desk-content ui-directory">{filtered.map(agent => <div className="ui-row" key={agent.id}>
      <AgentAvatar agent={agent} /><div className="ui-row-title"><strong>{agent.name}</strong><small>{agent.role}</small></div>
      <span className="ui-badge">{agent.scope === 'private' ? 'Only you' : 'Workspace'}</span>
      <span className="agent-status">{agent.settings.paused ? 'Paused' : agent.pending ? `${agent.pending} pending` : 'Idle'}</span>
      <Button disabled={mutation.busy || agent.settings.paused} onClick={() => void mutation.run(async () => { const result = await identityRequest<{ id: string }>(`/workspaces/${workspaceId}/chats`, { id: crypto.randomUUID(), kind: 'direct', name: agent.name, description: '', users: [], agents: [agent.id] }); if (alive.current) location.assign(workspaceUrl(`/?view=chat&conversation=${result.id}`)); })}>Message</Button>
      <Button aria-label={`View ${agent.name}`} onClick={() => select(agent)}>Details</Button>
      <Button aria-label={`Tools and approvals for ${agent.name}`} onClick={() => control(agent)}>Tools and approvals</Button>
    </div>)}{data && !filtered.length && <p className="ui-empty">{query ? 'No matching agents.' : 'Create an agent and give it a responsibility.'}</p>}{currentWorkspace()?.role === 'Owner' && !workspaceSession?.simulation && <LocalAgents query={query} />}</div>
    {selected && data && <AgentEditor creating={!data.agents.some(a => a.id === selected.id)} key={selected.id} agent={selected} directory={data} close={() => select(undefined)} saved={async () => { select(undefined); await refresh(); }} />}
    {controlled && <AgentControl key={controlled.id} agent={controlled} close={() => control(undefined)} />}
  </main></div>;
}
