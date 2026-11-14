import { useState } from 'react';
import { chatRequest } from '../chat/model';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { loadSettings } from './workspace-model';
import { WorkspaceRules } from './workspace-rules';
import { WorkspaceDeletion } from './workspace-deletion';
import { PauseAgentsControl } from '../activity/agents-pause';
import { loadCatalog } from '../agents/catalog-model';
import { WorkspaceTelegram } from './workspace-telegram';

/** Settings → Workspace (designs/settings 4): name, rules and the danger zone. */
export function WorkspaceGeneral({ renamed }: { renamed: (name: string) => void }) {
  const resource = useResource(loadSettings);
  // Loaded alongside the settings; without it the model list keeps naming providers.
  const catalog = useResource(loadCatalog);
  const data = resource.data;
  const catalogSettled = Boolean(catalog.data || catalog.error);
  return <section className="settings-workspace"><header className="settings-section-heading"><div><h2>Workspace</h2>
    <p>Name, look and rules for the whole workspace.</p></div><span className="ui-badge">Admins only</span></header>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!data && !resource.error && <p role="status">Loading workspace…</p>}
    {data && <>
      <GeneralForm initial={data.name} renamed={renamed} />
      {catalogSettled ? <WorkspaceRules initial={data} catalog={catalog.data?.catalog_on ? catalog.data : undefined} saved={next => resource.setData(next)} />
        : <p role="status">Loading models…</p>}
      <section className="settings-pause" aria-label="Pause all agents">
        <div><h3>Pause all agents</h3><p>If an agent is doing something wrong, stop every agent at once. Resume when it’s fixed.</p></div>
        <PauseAgentsControl />
      </section>
      <WorkspaceTelegram />
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
