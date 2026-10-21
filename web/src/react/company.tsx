import { SimulationReport } from './simulation-report';
import { CompanyWork } from './company-work';
import { useCallback, useRef, useState } from 'react';
import { chatRequest, type Directory } from './chat/model';
import { currentWorkspace, workspaceSession } from '../workspace-context';
import { Button, Dialog } from './ui';
import { useMutation, useResource } from './requests';
type Team = { id: string; name: string; users: string[] };
export function CompanyPage() {
  const load = useCallback(async () => {
    const [teams, directory] = await Promise.all([chatRequest<{ teams: Team[] }>('/teams'), chatRequest<Directory>('/chat-directory')]); return { ...teams, directory };
  }, []);
  const resource = useResource(load);
  const [team, setTeam] = useState<Team>();
  const [agent, setAgent] = useState(false);
  const manager = ['Owner', 'Admin'].includes(currentWorkspace()?.role ?? '');
  return <main className="company-page"><header><h1>Company</h1>{manager && <Button onClick={() => setTeam({ id: crypto.randomUUID(), name: '', users: [] })}>Create team</Button>}</header>
    {workspaceSession?.simulation && currentWorkspace()?.role === 'Owner' && <SimulationReport />}
    <p>Teams organize employees. Channel membership and permission to request an agent remain explicit.</p>
    {resource.error && <p role="alert">{resource.error}</p>}
    <div className="company-rows">{!resource.error && resource.data?.teams.map(t => <div key={t.id}><span><strong>{t.name}</strong><small>{t.users.map(id => resource.data?.directory.users.find(u => u.id === id)?.name ?? 'Former member').join(', ')}</small></span>{manager && <Button onClick={() => setTeam(t)}>Edit team</Button>}</div>)}</div>
    <header><h2>Shared agents</h2>{currentWorkspace()?.role === 'Owner' && <Button onClick={() => setAgent(true)}>Create shared agent</Button>}</header>
    <p>Shared agents use conversation context and do not need a private computer. Business operations are not enabled by creating an agent.</p>
    <div className="company-rows">{!resource.error && resource.data?.directory.agents.map(a => <div key={a.id}><strong>{a.name}</strong></div>)}</div>
    {resource.data && !resource.error && <CompanyWork directory={resource.data.directory} />}
    {team && resource.data && <TeamForm team={team} directory={resource.data.directory} close={() => setTeam(undefined)} saved={async () => { await resource.refresh(); setTeam(undefined); }} />}
    {agent && <AgentForm close={() => setAgent(false)} saved={async () => { await resource.refresh(); setAgent(false); }} />}
  </main>;
}
function TeamForm({ team, directory, close, saved }: { team: Team; directory: Directory; close: () => void; saved: () => Promise<void> }) {
  const [draft, setDraft] = useState(team); const mutation = useMutation();
  return <Dialog title="Team" close={close} busy={mutation.busy} status={mutation.status}><form className="company-form" onSubmit={e => { e.preventDefault(); void mutation.run(async () => { await chatRequest('/teams', draft); await saved(); }); }}>
    <label>Name<input required maxLength={80} value={draft.name} onChange={e => setDraft({ ...draft, name: e.target.value })} /></label>
    {directory.users.map(u => <label key={u.id}><input type="checkbox" checked={draft.users.includes(u.id)} onChange={e => setDraft({ ...draft, users: e.target.checked ? [...draft.users, u.id] : draft.users.filter(id => id !== u.id) })} />{u.name}</label>)}
    <Button primary type="submit" disabled={mutation.busy}>Save team</Button></form></Dialog>;
}
function AgentForm({ close, saved }: { close: () => void; saved: () => Promise<void> }) {
  const [name, setName] = useState(''); const [role, setRole] = useState(''); const [model, setModel] = useState('');
  const mutation = useMutation(); const attempt = useRef<{ payload: string; id: string }>(undefined);
  return <Dialog title="Shared agent" close={close} busy={mutation.busy} status={mutation.status}><form className="company-form" onSubmit={e => { e.preventDefault(); void mutation.run(async () => {
    const payload = JSON.stringify({ name, role, model }); if (attempt.current?.payload !== payload) attempt.current = { payload, id: crypto.randomUUID() };
    await chatRequest('/shared-agents', { id: attempt.current.id, name, role, model }); await saved();
  }); }}><label>Name<input required maxLength={80} value={name} onChange={e => setName(e.target.value)} /></label><label>Responsibilities<textarea maxLength={4000} value={role} onChange={e => setRole(e.target.value)} /></label><label>Configured model<input required maxLength={200} value={model} onChange={e => setModel(e.target.value)} placeholder="Use scripted in the simulation" /></label><Button primary type="submit" disabled={mutation.busy}>Create agent</Button></form></Dialog>;
}
