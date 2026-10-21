import { useEffect, useState } from 'react';
import { api } from '../../types';
import { workspaceUrl } from '../../workspace-context';
import type { Profile } from '../../agents';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
type Directory = { creation_available: boolean; agents: { profile: Profile; activity: { pending_tasks: number } }[]; memory_available: boolean };
const load = () => api<Directory>('/agents', undefined, 'local-agent');
const post = (path: string, body: unknown) => api(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }, 'local-agent');

/** Existing VM-backed agents keep their files, tools and lifecycle during migration. */
export function LocalAgents({ query }: { query: string }) {
  const { data, error, refresh } = useResource(load);
  const [selected, select] = useState<Profile>();
  useEffect(() => { const timer = window.setInterval(() => { if (!document.hidden) void refresh().catch(() => {}); }, 10000); return () => clearInterval(timer); }, [refresh]);
  return <section aria-label="Existing local agents"><h2 className="agent-section-title">Existing local agents</h2><p className="ui-help">Their saved conversations, files, browser and connections are preserved.</p>{data?.creation_available && <a className="ui-button" href={workspaceUrl('/?view=agents&legacy=1')}>Create a local agent</a>}
    {error && <p role="alert">{error}<Button onClick={() => void refresh().catch(() => {})}>Retry local agents</Button></p>}
    {data?.agents.filter(({ profile }) => `${profile.name} ${profile.role}`.toLowerCase().includes(query.toLowerCase())).map(({ profile, activity }) => <div className="ui-row" key={profile.id}><span className="ui-avatar">✦</span><div className="ui-row-title"><strong>{profile.name}</strong><small>{profile.role}</small></div><span className="ui-badge">Local</span><span className="agent-status">{activity.pending_tasks ? `${activity.pending_tasks} pending` : profile.state}</span><a className="ui-button" href={workspaceUrl(`/?view=chat&agent=${encodeURIComponent(profile.id)}`)}>Message</a><Button aria-label={`View local ${profile.name}`} onClick={() => select(profile)}>Details</Button></div>)}
    {selected && data && <LocalEditor key={selected.id} profile={selected} memoryAvailable={data.memory_available} close={() => select(undefined)} saved={async () => { select(undefined); await refresh(); }} />}
  </section>;
}
function LocalEditor({ profile, memoryAvailable, close, saved }: { profile: Profile; memoryAvailable: boolean; close: () => void; saved: () => Promise<void> }) {
  const [draft, setDraft] = useState(profile);
  const mutation = useMutation();
  const actions = profile.id === 'local-agent' ? [] : profile.state === 'archived' ? ['restore', 'delete'] : profile.state === 'failed' ? ['retry', 'archive'] : ['ready', 'archiving'].includes(profile.state) ? ['archive'] : [];
  return <Dialog drawer title={profile.name} close={close} busy={mutation.busy} status={mutation.status}>
    <form className="ui-form" onSubmit={e => { e.preventDefault(); void mutation.run(async () => { await post(`/agents/${profile.id}/profile`, { name: draft.name, role: draft.role, model: draft.model, memory: draft.memory, browser: draft.browser, expected_revision: profile.revision }); await saved(); }); }}>
      <fieldset className="ui-fieldset" disabled={!profile.editable}>{(['name', 'role', 'model'] as const).map(key => <label className="ui-field" key={key}>{key === 'role' ? 'Responsibility' : key === 'model' ? 'Model identifier' : 'Name'}<input required value={draft[key]} onChange={e => setDraft({ ...draft, [key]: e.target.value })} /></label>)}
        <label><input type="checkbox" disabled={!memoryAvailable && !profile.memory} checked={draft.memory} onChange={e => setDraft({ ...draft, memory: e.target.checked })} /> Memory</label>
        <label><input type="checkbox" disabled={!profile.browser_available && !profile.browser} checked={draft.browser} onChange={e => setDraft({ ...draft, browser: e.target.checked })} /> Browser access</label>
        {profile.editable && <Button type="submit" primary disabled={mutation.busy}>Save local agent</Button>}
      </fieldset>
      <p className="ui-help">{profile.state}{profile.detail ? ` · ${profile.detail}` : ''}</p>
      {actions.map(action => <Button key={action} disabled={mutation.busy} onClick={() => { if (['delete', 'archive'].includes(action) && !confirm(`${action === 'archive' ? 'Archive' : 'Remove'} this agent? Current work may stop. Saved files, history and Apps are retained.`)) return; void mutation.run(async () => { await post(`/agents/${profile.id}/${action === 'retry' ? 'retry' : 'lifecycle'}`, action === 'retry' ? {} : { action }); await saved(); }); }}>{action[0].toUpperCase() + action.slice(1)} agent</Button>)}
    </form>
  </Dialog>;
}
