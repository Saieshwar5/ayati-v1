import { useCallback, useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { defaults, loadAgents, type Agent } from './model';
import { AgentCreate, newAgent } from './create/create-flow';
import { agentFrom, discardDraft, keepDraft, loadChatDraft, loadSavedDraft, type SavedDraft } from './create/saved-draft';
import { identityRequest, storageScope, workspaceId, workspaceUrl, workspaceSession } from '../../workspace-context';
import type { Conversation } from '../chat/model';
import { AgentAvatar } from './avatar';
import { AgentRoster } from './agent-roster';
import { AgentWorkspace } from './workspace';
import { sectionNamed, type WorkspaceSection } from './workspace-tabs';
import { subscribeChanges } from '../chat/live';
import './workspace.css';

export function AgentsPage() {
  const { data, error, refresh, setData } = useResource(loadAgents);
  const mutation = useMutation();
  const epoch = useRef(0);
  useEffect(() => () => { epoch.current++; }, []);
  const initial = new URLSearchParams(location.search).get('selected');
  // A link may open one tab of the agent it selects (agentSectionUrl).
  const linkedSection = initial ? sectionNamed(new URLSearchParams(location.search).get('section')) : undefined;
  const [linkedAction] = useState(() => (initial && new URLSearchParams(location.search).get('action')) || undefined);
  const selectionKey = storageScope('relay.agents.selection');
  const [selectedKey, setSelected] = useState<string | undefined>(() => {
    if (initial) return `m:${initial}`;
    try { return window.sessionStorage.getItem(selectionKey) ?? undefined; } catch { return undefined; }
  });
  const [showProfile, setShowProfile] = useState(Boolean(selectedKey));
  const savedSection = (agentKey?: string): WorkspaceSection => {
    try { const value = window.sessionStorage.getItem(`${selectionKey}.section.${agentKey}`); return sectionNamed(value) ?? 'Overview'; } catch { return 'Overview'; }
  };
  const [section, setSection] = useState<WorkspaceSection>(() => linkedSection ?? savedSection(selectedKey));
  // New agent: a fresh one, or the one this person had started and not created (schema 139).
  const kept = useResource(loadSavedDraft);
  const [creating, setCreating] = useState<{ agent: Agent; resume?: SavedDraft }>();
  const begin = () => {
    if (!data) return;
    const fresh = newAgent(data);
    setCreating(kept.data ? { agent: agentFrom(kept.data, fresh), resume: kept.data } : { agent: fresh });
    setShowProfile(true);
  };
  const ended = () => { setCreating(undefined); kept.setData(undefined); };
  // A link from a draft your assistant made in chat ("Review in New agent") opens it here.
  const linkedDraft = new URLSearchParams(location.search).get('setup');
  const fromChat = useResource(useCallback(async () => linkedDraft ? loadChatDraft(linkedDraft) : undefined, [linkedDraft]));
  const ready = Boolean(data);
  useEffect(() => {
    if (!fromChat.data || !data) return;
    setCreating({ agent: agentFrom(fromChat.data, newAgent(data)), resume: fromChat.data });
    setShowProfile(true);
  // Opened once per draft, when both it and the directory are here.
  }, [fromChat.data, ready]);
  // A "Create agent" link (Chat's start tiles: ?view=agents&new=1) opens the create
  // steps once, resuming this person's unfinished agent if they have one. The flag
  // leaves the address at once, so Back and reload do not open them again.
  const [linkedNew] = useState(() => !linkedDraft && new URLSearchParams(location.search).get('new') === '1');
  useEffect(() => {
    if (!linkedNew) return;
    const url = new URL(window.location.href); url.searchParams.delete('new'); window.history.replaceState(null, '', url);
  }, [linkedNew]);
  const openedNew = useRef(false);
  useEffect(() => {
    if (!linkedNew || !data || openedNew.current) return;
    openedNew.current = true;
    const generation = epoch.current;
    void loadSavedDraft().catch(() => undefined).then(saved => {
      if (generation !== epoch.current) return;
      const fresh = newAgent(data);
      setCreating(saved ? { agent: agentFrom(saved, fresh), resume: saved } : { agent: fresh });
      setShowProfile(true);
    });
  // Opened once, when the directory is here.
  }, [linkedNew, ready]);
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
      if (agent.needs_owner) throw new Error('This agent’s last owner left the workspace, so it is paused. An admin needs to give it a new owner.');
      if (agent.settings.paused) throw new Error('This agent is paused and has no direct conversation yet. Resume it before starting a conversation.');
      const result = await identityRequest<{ id: string }>(root, { id: crypto.randomUUID(), kind: 'direct', name: agent.name, description: '', users: [], agents: [agent.id] });
      chat = { id: result.id } as Conversation;
    }
    if (generation === epoch.current) location.assign(workspaceUrl(`/?view=chat&conversation=${chat.id}`));
  };
  return <div id="app" className={`agents-page${showProfile ? ' show-agent-profile' : ''}`}>
    <AgentRoster agents={agents} loaded={Boolean(data)} error={error || undefined} retry={() => void refresh().catch(() => {})}
      current={selected?.id} locked={Boolean(creating)} begin={begin} select={id => select(`m:${id}`)}
      draft={creating ? <Button className="agent-team-row" aria-current="true" onClick={() => setShowProfile(true)}>
        <AgentAvatar agent={creating.agent} /><span><strong>New agent</strong><small>Not created yet</small></span>
      </Button> : kept.data && data ? <Button className="agent-team-row" aria-label={`Continue setting up ${kept.data.name || 'your new agent'}`} onClick={begin}>
        <AgentAvatar agent={agentFrom(kept.data, newAgent(data))} /><span><strong>{kept.data.name || 'New agent'}</strong><small>Not created yet · Continue</small></span>
      </Button> : undefined} />
    <main className="agent-selected" aria-label="Selected agent">
      {mutation.status && <p className="agent-page-error" role="alert">{mutation.status}</p>}
      {creating && data && <AgentCreate key={creating.agent.id} start={creating.agent} resume={creating.resume} keep={keepDraft} discard={discardDraft} close={ended}
        created={async id => { ended(); await refresh(); select(`m:${id}`); }} />}
      {!creating && selected && data && <AgentWorkspace key={selected.id} agent={selected} directory={data} focus={`m:${selected.id}` === `m:${initial}` ? linkedAction : undefined} section={section} selectSection={selectSection} back={back} openChat={() => void mutation.run(() => openChat(selected))} busy={mutation.busy} saved={async () => { await refresh(); }} />}
      {data && !creating && !selected && <div className="agent-workspace-empty"><h2>{key ? 'Agent unavailable' : 'Your virtual team'}</h2><p>{key ? 'Choose an agent you can currently access.' : 'Create an agent with a clear responsibility to get started.'}</p><Button className="agent-roster-back" onClick={back}>‹ Agents</Button></div>}
    </main>
  </div>;
}
