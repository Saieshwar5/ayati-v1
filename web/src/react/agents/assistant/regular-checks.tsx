import { useCallback, useState } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import './assistant.css';
import { loadAssistantSettings, saveAssistantSettings } from './settings-model';
import { taskUrl } from '../../activity/model';

type Checks = { checks: boolean; every_minutes: number; checklist: string };
const intervals: [number, string][] = [[30, 'Every 30 minutes'], [60, 'Every hour'], [120, 'Every 2 hours'], [240, 'Every 4 hours'], [480, 'Every 8 hours'], [1440, 'Once a day']];

/** Regular checks, first on your assistant's Automations tab: it looks at new
 * direct messages and mentions for you and says what needs you. The check is a
 * task (schema 149): saving here changes it, and its next run shows here. */
export function RegularChecks({ name }: { name: string }) {
  const load = useCallback(() => loadAssistantSettings(), []);
  const { data, error, refresh } = useResource(load);
  const [saved, setSaved] = useState<Checks>();
  const [draft, setDraft] = useState<Checks>();
  const mutation = useMutation();
  if (error) return <p role="alert">{error}</p>;
  if (!data || data.available === false) return null;
  const base = saved ?? { checks: data.checks, every_minutes: data.every_minutes, checklist: data.checklist };
  const form = draft ?? base;
  const changed = form.checks !== base.checks || form.every_minutes !== base.every_minutes || form.checklist !== base.checklist;
  const edit = (change: Partial<Checks>) => { mutation.setStatus(''); setDraft({ ...form, ...change }); };
  const save = () => void mutation.run(async () => {
    await saveAssistantSettings(form);
    setSaved(form); setDraft(undefined);
    void refresh().catch(() => {});
    mutation.setStatus(form.checks ? 'Saved. Regular checks are on.' : 'Saved. Regular checks are off.');
  });
  return <section className="assistant-checks" aria-label="Regular checks">
    <header className="agent-section-heading"><h3>Regular checks</h3></header>
    <label className="ui-check"><input type="checkbox" checked={form.checks} onChange={event => edit({ checks: event.target.checked })} />
      <span>Check for me regularly<small className="ui-help">{name} looks at new direct messages and mentions for you and tells you what needs you, in your chat with it. When there is nothing new, it stays quiet. It answers nobody.</small></span></label>
    {form.checks && <>
      <label className="ui-field">How often<select value={form.every_minutes} onChange={event => edit({ every_minutes: Number(event.target.value) })}>
        {intervals.map(([minutes, label]) => <option key={minutes} value={minutes}>{label}</option>)}</select></label>
      <label className="ui-field">Also check every time (optional)
        <textarea rows={3} maxLength={3000} value={form.checklist} onChange={event => edit({ checklist: event.target.value })}
          placeholder="For example: are any of my open orders late?" /></label>
    </>}
    {base.checks && !changed && data.task?.next_text && <p className="ui-help">Next check {data.task.next_text} · <a href={taskUrl(data.task.id)}>Open its task</a></p>}
    {!base.checks && data.task?.paused && <p className="ui-help">Paused. Turn it on and save to start again.</p>}
    <div className="agent-save-row">
      <Button primary disabled={mutation.busy || !changed} onClick={save}>Save changes</Button>
      {changed && <Button disabled={mutation.busy} onClick={() => { setDraft(undefined); mutation.setStatus(''); }}>Discard</Button>}
      <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    </div>
  </section>;
}
