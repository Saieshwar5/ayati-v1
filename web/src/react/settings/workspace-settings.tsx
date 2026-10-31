import { useState } from 'react';
import { chatRequest } from '../chat/model';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { loadSettings } from './workspace-model';
import { WorkspaceRules } from './workspace-rules';
import { WorkspaceDeletion } from './workspace-deletion';

/** Settings → Workspace (designs/settings 4): name, rules and the danger zone. */
export function WorkspaceGeneral({ renamed }: { renamed: (name: string) => void }) {
  const resource = useResource(loadSettings);
  const data = resource.data;
  return <section className="settings-workspace"><header className="settings-section-heading"><div><h2>Workspace</h2>
    <p>Name, look and rules for the whole workspace.</p></div><span className="ui-badge">Admins only</span></header>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!data && !resource.error && <p role="status">Loading workspace…</p>}
    {data && <>
      <GeneralForm initial={data.name} renamed={renamed} />
      <WorkspaceRules initial={data} saved={next => resource.setData(next)} />
      <WorkspaceDeletion settings={data} changed={resource.refresh} />
    </>}
  </section>;
}
function GeneralForm({ initial, renamed }: { initial: string; renamed: (name: string) => void }) {
  const [name, setName] = useState(initial);
  const [saved, setSaved] = useState(initial);
  const mutation = useMutation();
  return <form className="ui-form settings-general" onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
    const result = await chatRequest<{ name: string }>('/settings', { name, previous_name: saved });
    setSaved(result.name); setName(result.name); renamed(result.name); mutation.setStatus('Workspace name saved.');
  }); }}><fieldset className="ui-fieldset" disabled={mutation.busy}><label className="ui-field">Workspace name<input required maxLength={80} value={name} onChange={event => setName(event.target.value)} /></label>
    <p className="ui-help">Admins manage this workspace. Members use what is shared with them.</p><Button type="submit" primary disabled={mutation.busy}>{mutation.busy ? 'Saving…' : 'Save changes'}</Button></fieldset><p role="status">{mutation.status}</p></form>;
}
