import { useId, useState } from 'react';
import { chatRequest, type Directory } from '../chat/model';
import { useMutation, useResource } from '../requests';
import { loadSettings, saveSettings, settingsInput, timeIn, timeZones, type WorkspaceSettings } from './workspace-model';
import { WorkspaceDeletion } from './workspace-deletion';
import { WorkspaceCalendarSettings } from './workspace-calendar';
import { SaveBar } from './save-bar';
import { WorkspaceCard, count } from './this-workspace';

const loadDirectory = () => chatRequest<Directory>('/chat-directory');

/** Settings → General: the workspace's name, time zone, working days and holidays,
 * and deleting it. What
 * agents may do (models, approval backup, pausing) is on Agent rules. */
export function WorkspaceGeneral({ renamed }: { renamed: (name: string) => void }) {
  const resource = useResource(loadSettings);
  const directory = useResource(loadDirectory).data;
  const data = resource.data;
  return <section className="settings-workspace"><h2>General</h2>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!data && !resource.error && <p role="status">Loading workspace…</p>}
    {data && <>
      <WorkspaceCard name={data.name} line={directory?.users && directory.agents ? `${count(directory.users.length, 'person', 'people')} · ${count(directory.agents.length, 'agent', 'agents')}` : 'You are an admin'} />
      <GeneralForm settings={data} saved={next => { resource.setData(next); renamed(next.name); }} />
      <WorkspaceCalendarSettings />
      <WorkspaceDeletion settings={data} changed={resource.refresh} />
    </>}
  </section>;
}

/** Name and time zone under one Save. They are two writes; each is kept as soon as
 * it succeeds, so a failure leaves only what did not save as a draft. */
function GeneralForm({ settings, saved }: { settings: WorkspaceSettings; saved: (next: WorkspaceSettings) => void }) {
  const [name, setName] = useState(settings.name);
  const [zone, setZone] = useState(settings.timezone ?? '');
  const [zones] = useState(() => timeZones(settings.timezone));
  const mutation = useMutation();
  const form = useId();
  const renamed = name.trim() !== settings.name;
  const moved = zone !== (settings.timezone ?? '');
  const save = () => void mutation.run(async () => {
    let current = settings;
    if (renamed) {
      const result = await chatRequest<{ name: string }>('/settings', { name, previous_name: settings.name });
      current = { ...current, name: result.name };
      saved(current); setName(result.name);
    }
    if (moved && zone) saved({ ...await saveSettings(settingsInput(current, { timezone: zone })), name: current.name });
    mutation.setStatus('Saved.');
  });
  return <form id={form} className="ui-form settings-general" onSubmit={event => { event.preventDefault(); if (name.trim() && (renamed || moved)) save(); }}>
    <fieldset className="ui-fieldset" disabled={mutation.busy}>
      <label className="ui-field">Workspace name<input name="name" required maxLength={80} value={name} onChange={event => setName(event.target.value)} /></label>
      <label className="ui-field">Time zone<select name="timezone" value={zone} onChange={event => setZone(event.target.value)}>
        {!settings.timezone && <option value="">Choose a time zone…</option>}
        {zones.map(choice => <option key={choice.id} value={choice.id}>{choice.label}</option>)}</select></label>
      <p className="ui-help">Agent tasks, schedules and the clock on Home use this. India time unless you choose another.{zone && timeIn(zone) ? ` It is ${timeIn(zone)} there now.` : ''}</p>
    </fieldset>
    {(renamed || moved) && <SaveBar form={form} busy={mutation.busy} disabled={!name.trim()} discard={() => { setName(settings.name); setZone(settings.timezone ?? ''); mutation.setStatus(''); }} />}
    <p className="ui-status" role="status">{mutation.status}</p>
  </form>;
}
