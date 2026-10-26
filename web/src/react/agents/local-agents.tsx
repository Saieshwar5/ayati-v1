import { useState } from 'react';
import { api } from '../../types';
import type { Profile } from '../../agents';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
export type LocalDirectory = { creation_available: boolean; agents: { profile: Profile; activity: { pending_tasks: number } }[]; memory_available: boolean };
export const loadLocalAgents = () => api<LocalDirectory>('/agents', undefined, 'local-agent');
const post = (path: string, body: unknown) => api(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }, 'local-agent');

export function LocalEditor({ profile, memoryAvailable, close, saved }: { profile: Profile; memoryAvailable: boolean; close: () => void; saved: () => Promise<void> }) {
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
