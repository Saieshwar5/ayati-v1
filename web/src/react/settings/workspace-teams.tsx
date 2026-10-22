import { SimulationReport } from '../simulation-report';
import { useCallback, useState } from 'react';
import { chatRequest, type Directory } from '../chat/model';
import { workspaceSession } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
type Team = { id: string; name: string; users: string[] };
export function WorkspaceTeams() {
  const resource = useResource(useCallback(async () => {
    const [teams, directory] = await Promise.all([chatRequest<{ teams: Team[] }>('/teams'), chatRequest<Directory>('/chat-directory')]);
    return { ...teams, directory };
  }, []));
  const [team, setTeam] = useState<Team>();
  return <section><header className="settings-section-heading"><div><h2>Teams</h2><p>Optional groups of people. Conversation and agent access remain explicit.</p></div><Button onClick={() => setTeam({ id: crypto.randomUUID(), name: '', users: [] })}>Create team</Button></header>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!resource.error && resource.data?.teams.map(t => <div className="ui-row" key={t.id}><div className="ui-row-title"><strong>{t.name}</strong><small>{t.users.map(id => resource.data?.directory.users.find(u => u.id === id)?.name ?? 'Former user').join(', ')}</small></div><Button onClick={() => setTeam(t)}>Edit team</Button></div>)}
    {team && resource.data && <TeamForm team={team} directory={resource.data.directory} close={() => setTeam(undefined)} saved={async () => { await resource.refresh(); setTeam(undefined); }} />}
    {workspaceSession?.simulation && <SimulationReport />}
  </section>;
}
function TeamForm({ team, directory, close, saved }: { team: Team; directory: Directory; close: () => void; saved: () => Promise<void> }) {
  const [draft, setDraft] = useState(team); const mutation = useMutation();
  return <Dialog title="Team" close={close} busy={mutation.busy} status={mutation.status}><form className="company-form" onSubmit={e => { e.preventDefault(); void mutation.run(async () => { await chatRequest('/teams', draft); await saved(); }); }}>
    <label>Name<input required maxLength={80} value={draft.name} onChange={e => setDraft({ ...draft, name: e.target.value })} /></label>
    {directory.users.map(u => <label key={u.id}><input type="checkbox" checked={draft.users.includes(u.id)} onChange={e => setDraft({ ...draft, users: e.target.checked ? [...draft.users, u.id] : draft.users.filter(id => id !== u.id) })} />{u.name}</label>)}
    <Button primary type="submit" disabled={mutation.busy}>Save team</Button></form></Dialog>;
}
