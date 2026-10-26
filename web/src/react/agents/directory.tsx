import { useCallback, useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { AgentEditor } from './editor';
import { defaults, loadAgents, type Agent } from './model';
import { currentWorkspace, identityRequest, storageScope, workspaceId, workspaceUrl, workspaceSession } from '../../workspace-context';
import type { Profile } from '../../agents';
import type { Conversation } from '../chat/model';
import { loadLocalAgents, LocalEditor, type LocalDirectory } from './local-agents';
import { AgentControl } from './control';
import { GuidedSetup } from './guided-setup';
import { AgentAvatar } from './avatar';
import { AgentWorkspace } from './workspace';
import { LocalAgentWorkspace } from './local-workspace';
import { workspaceSections, type WorkspaceSection } from './workspace-tabs';
import { subscribeChanges } from '../chat/live';
import { RehearsalLauncher } from './rehearsals/launcher';
import './workspace.css';

export function AgentsPage() {
  const { data, error, refresh, setData } = useResource(loadAgents);
  const allowLocal = currentWorkspace()?.role === 'Owner' && !workspaceSession?.simulation;
  const loadLocal = useCallback(() => allowLocal ? loadLocalAgents() : Promise.resolve<LocalDirectory>({ agents: [], creation_available: false, memory_available: false }), [allowLocal]);
  const local = useResource(loadLocal);
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
  const [filter, setFilter] = useState('all');
  const [editing, edit] = useState<Agent>();
  const [editingLocal, editLocal] = useState<Profile>();
  const [controlled, control] = useState<Agent>();
  const [guided, guide] = useState<Agent>();
  const [rehearsing, rehearse] = useState<Agent>();
  const [revision, setRevision] = useState(0);
  useEffect(() => subscribeChanges(change => {
    if (change.kind === 'reset' || change.kind === 'event_access') { void refresh().catch(() => {}); return; }
    if (change.kind !== 'access') return;
    epoch.current++; setData(undefined); local.setData(undefined);
    edit(undefined); editLocal(undefined); guide(undefined); control(undefined);
    rehearse(undefined);
    void refresh().catch(() => {}); if (allowLocal) void local.refresh().catch(() => {});
  }), [setData, local.setData, refresh, local.refresh, allowLocal]);
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (!document.hidden) { void refresh().catch(() => {}); if (allowLocal) void local.refresh().catch(() => {}); }
    }, 10000);
    return () => clearInterval(timer);
  }, [refresh, local.refresh, allowLocal]);
  const agents = (data?.agents ?? []).map(agent => ({ ...agent, settings: { ...defaults, ...agent.settings } }));
  const locals = local.error ? [] : local.data?.agents ?? [];
  const key = selectedKey ?? (data ? agents[0] ? `m:${agents[0].id}` : locals[0] ? `l:${locals[0].profile.id}` : undefined : undefined);
  const selected = agents.find(a => `m:${a.id}` === key);
  const selectedLocal = locals.find(a => `l:${a.profile.id}` === key)?.profile;
  const match = (name: string, role: string) => `${name} ${role}`.toLowerCase().includes(query.toLowerCase());
  const filtered = agents.filter(a => (filter === 'all' || a.scope === filter) && match(a.name, a.role));
  const localFiltered = filter === 'workspace' ? [] : locals.filter(a => match(a.profile.name, a.profile.role));
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
  const closeControls = () => { guide(undefined); control(undefined); setRevision(n => n + 1); void refresh().catch(() => {}); };
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
      <Button className="ui-icon" aria-label="＋ Create agent" title="Create agent" disabled={!data} onClick={() => edit({ id: crypto.randomUUID(), name: '', role: '', model: data!.default_model, scope: 'private', revision: 0, settings: { ...defaults }, editable: true })}>＋</Button>
    </header>
    <input className="ui-search" type="search" aria-label="Find an agent" placeholder="Search agents" value={query} onChange={e => setQuery(e.target.value)} />
    <div className="agent-roster-filters" aria-label="Agent visibility">{[['all', 'All'], ['private', 'Private'], ['workspace', 'Workspace']].map(([value, label]) => <Button key={value} aria-pressed={filter === value} onClick={() => setFilter(value)}>{label}</Button>)}</div>
    {error && <p role="alert">{error}<Button onClick={() => void refresh().catch(() => {})}>Retry agents</Button></p>}
    {!data && !error && <p role="status">Loading agents…</p>}
    <div className="agent-roster-scroll"><h2>Your team <small>{filtered.length}</small></h2>
      {filtered.map(agent => <Button className="agent-team-row" key={agent.id} aria-label={`Open agent ${agent.name}`} aria-current={key === `m:${agent.id}` ? 'true' : undefined} onClick={() => select(`m:${agent.id}`)}>
        <AgentAvatar agent={agent} /><span><strong>{agent.name}</strong><small>{agent.role}</small><small className="agent-roster-state">{agent.settings.paused ? 'Paused' : agent.pending ? `${agent.pending} visible requests` : agent.scope === 'private' ? 'Only you' : 'Workspace'}</small></span>
      </Button>)}
      {data && !filtered.length && <p className="ui-empty">{query ? 'No matching agents.' : 'Create an agent and give it a responsibility.'}</p>}
      {allowLocal && <><h2>Existing local agents</h2>{local.error && <p role="alert">{local.error}<Button onClick={() => void local.refresh().catch(() => {})}>Retry local agents</Button></p>}
        {localFiltered.map(({ profile }) => <Button className="agent-team-row" key={profile.id} aria-label={`Open local agent ${profile.name}`} aria-current={key === `l:${profile.id}` ? 'true' : undefined} onClick={() => select(`l:${profile.id}`)}><span className="ui-avatar">✦</span><span><strong>{profile.name}</strong><small>{profile.role}</small><small className="agent-roster-state">Local · {profile.state}</small></span></Button>)}
        {local.data?.creation_available && <a className="ui-button" href={workspaceUrl('/?view=agents&legacy=1')}>Create a local agent</a>}</>}
    </div><footer>People, purpose and controlled work.</footer></aside>
    <main className="agent-selected" aria-label="Selected agent">
      {mutation.status && <p className="agent-page-error" role="alert">{mutation.status}</p>}
      {selected && <AgentWorkspace key={selected.id} agent={selected} revision={revision} section={section} selectSection={selectSection} back={back} edit={() => edit(selected)} guide={() => guide(selected)} control={() => control(selected)} rehearse={() => rehearse(selected)} openChat={() => void mutation.run(() => openChat(selected))} busy={mutation.busy} />}
      {selectedLocal && <LocalAgentWorkspace key={selectedLocal.id} profile={selectedLocal} section={section} selectSection={selectSection} back={back} edit={() => editLocal(selectedLocal)} />}
      {data && !selected && !selectedLocal && <div className="agent-workspace-empty"><h2>{key ? 'Agent unavailable' : 'Your virtual team'}</h2><p>{key ? 'Choose an agent you can currently access.' : 'Create an agent with a clear responsibility to get started.'}</p><Button className="agent-roster-back" onClick={back}>‹ Agents</Button></div>}
    </main>
    {editing && data && <AgentEditor creating={!data.agents.some(a => a.id === editing.id)} key={editing.id} agent={editing} directory={data} close={() => edit(undefined)} saved={async () => { const id = editing.id; edit(undefined); await refresh(); select(`m:${id}`); setRevision(n => n + 1); }} />}
    {editingLocal && local.data && <LocalEditor key={editingLocal.id} profile={editingLocal} memoryAvailable={local.data.memory_available} close={() => editLocal(undefined)} saved={async () => { editLocal(undefined); await local.refresh(); }} />}
    {guided && <GuidedSetup key={guided.id} agent={guided} close={closeControls} />}
    {controlled && <AgentControl key={controlled.id} agent={controlled} close={closeControls} />}
    {rehearsing && <RehearsalLauncher key={rehearsing.id} agent={rehearsing} close={() => rehearse(undefined)} />}
  </div>;
}
