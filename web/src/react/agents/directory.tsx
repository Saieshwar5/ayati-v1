import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { defaults, loadAgents, type Agent } from './model';
import { AgentCreate, newAgent } from './create/create-flow';
import { identityRequest, storageScope, workspaceId, workspaceUrl, workspaceSession } from '../../workspace-context';
import type { Conversation } from '../chat/model';
import { AgentAvatar } from './avatar';
import { AgentWorkspace } from './workspace';
import { workspaceSections, type WorkspaceSection } from './workspace-tabs';
import { subscribeChanges } from '../chat/live';
import './workspace.css';

export function AgentsPage() {
  const { data, error, refresh, setData } = useResource(loadAgents);
  const mutation = useMutation();
  const epoch = useRef(0);
  useEffect(() => () => { epoch.current++; }, []);
  const initial = new URLSearchParams(location.search).get('selected');
  const selectionKey = storageScope('relay.agents.selection');
  const [selectedKey, setSelected] = useState<string | undefined>(() => {
    if (initial) return `m:${initial}`;
    try { return window.sessionStorage.getItem(selectionKey) ?? undefined; } catch { return undefined; }
  });
  const [showProfile, setShowProfile] = useState(Boolean(selectedKey));
  const savedSection = (agentKey?: string): WorkspaceSection => {
    try { const value = window.sessionStorage.getItem(`${selectionKey}.section.${agentKey}`); return workspaceSections.find(section => section === value) ?? 'Overview'; } catch { return 'Overview'; }
  };
  const [section, setSection] = useState<WorkspaceSection>(() => savedSection(selectedKey));
  const [query, setQuery] = useState('');
  const [creating, setCreating] = useState<Agent>();
  useEffect(() => subscribeChanges(change => {
    if (change.kind === 'reset' || change.kind === 'event_access') { void refresh().catch(() => {}); return; }
    if (change.kind !== 'access') return;
    epoch.current++; setData(undefined);
    void refresh().catch(() => {});
  }), [setData, refresh]);
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (!document.hidden) void refresh().catch(() => {});
    }, 10000);
    return () => clearInterval(timer);
  }, [refresh]);
  const agents = (data?.agents ?? []).map(agent => ({ ...agent, settings: { ...defaults, ...agent.settings } }));
  const key = selectedKey ?? (data && agents[0] ? `m:${agents[0].id}` : undefined);
  const selected = agents.find(a => `m:${a.id}` === key);
  const match = (name: string, role: string) => `${name} ${role}`.toLowerCase().includes(query.toLowerCase());
  const filtered = agents.filter(a => match(a.name, a.role));
  const select = (next: string) => {
    epoch.current++; setSelected(next); setShowProfile(true);
    if (next !== key) setSection(savedSection(next));
    try { window.sessionStorage.setItem(selectionKey, next); } catch { /* Optional identity-scoped navigation preference. */ }
  };
  const selectSection = (next: WorkspaceSection) => {
    setSection(next);
    try { window.sessionStorage.setItem(`${selectionKey}.section.${key}`, next); } catch { /* Optional view preference. */ }
  };
  const back = () => setShowProfile(false);
  const openChat = async (agent: Agent) => {
    const generation = epoch.current;
    const root = `/workspaces/${workspaceId}/chats`;
    const { chats } = await identityRequest<{ chats: Conversation[] }>(root);
    if (generation !== epoch.current) return;
    let chat = chats.find(c => c.kind === 'direct' && c.agents.includes(agent.id) && c.users.includes(workspaceSession?.user?.id ?? ''));
    if (!chat) {
      if (agent.settings.paused) throw new Error('This agent is paused and has no direct conversation yet. Resume it before starting a conversation.');
      const result = await identityRequest<{ id: string }>(root, { id: crypto.randomUUID(), kind: 'direct', name: agent.name, description: '', users: [], agents: [agent.id] });
      chat = { id: result.id } as Conversation;
    }
    if (generation === epoch.current) location.assign(workspaceUrl(`/?view=chat&conversation=${chat.id}`));
  };
  return <div id="app" className={`agents-page${showProfile ? ' show-agent-profile' : ''}`}>
    <aside className="agent-roster" aria-label="Agent team"><header><h1>Agents</h1>
      <Button primary disabled={!data || Boolean(creating)} onClick={() => { setCreating(newAgent(data!)); setShowProfile(true); }}>＋ New agent</Button>
    </header>
    <input className="ui-search" type="search" aria-label="Find an agent" placeholder="Search agents" value={query} onChange={e => setQuery(e.target.value)} />
    {error && <p role="alert">{error}<Button onClick={() => void refresh().catch(() => {})}>Retry agents</Button></p>}
    {!data && !error && <p role="status">Loading agents…</p>}
    <div className="agent-roster-scroll"><h2>Your team <small>{filtered.length}</small></h2>
      {creating && <Button className="agent-team-row" aria-current="true" onClick={() => setShowProfile(true)}>
        <AgentAvatar agent={creating} /><span><strong>New agent</strong><small>Not created yet</small></span>
      </Button>}
      {filtered.map(agent => <Button className="agent-team-row" key={agent.id} aria-label={`Open agent ${agent.name}`} aria-current={!creating && key === `m:${agent.id}` ? 'true' : undefined} disabled={Boolean(creating)} onClick={() => select(`m:${agent.id}`)}>
        <AgentAvatar agent={agent} /><span><strong>{agent.name}</strong><small>{agent.role}</small>{(agent.settings.paused || Boolean(agent.pending)) && <small className="agent-roster-state">{agent.settings.paused ? 'Paused' : `${agent.pending} in progress`}</small>}</span>
      </Button>)}
      {data && !filtered.length && <p className="ui-empty">{query ? 'No matching agents.' : 'Create your first agent.'}</p>}
    </div><footer>People, purpose and controlled work.</footer></aside>
    <main className="agent-selected" aria-label="Selected agent">
      {mutation.status && <p className="agent-page-error" role="alert">{mutation.status}</p>}
      {creating && data && <AgentCreate key={creating.id} start={creating} directory={data} close={() => setCreating(undefined)}
        created={async id => { setCreating(undefined); await refresh(); select(`m:${id}`); }} />}
      {!creating && selected && data && <AgentWorkspace key={selected.id} agent={selected} directory={data} section={section} selectSection={selectSection} back={back} openChat={() => void mutation.run(() => openChat(selected))} busy={mutation.busy} saved={async () => { await refresh(); }} />}
      {data && !creating && !selected && <div className="agent-workspace-empty"><h2>{key ? 'Agent unavailable' : 'Your virtual team'}</h2><p>{key ? 'Choose an agent you can currently access.' : 'Create an agent with a clear responsibility to get started.'}</p><Button className="agent-roster-back" onClick={back}>‹ Agents</Button></div>}
    </main>
  </div>;
}
