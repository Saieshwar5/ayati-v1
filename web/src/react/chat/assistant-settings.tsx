import { useEffect, useState } from 'react';
import { Button, Dialog } from '../ui';
import { errorMessage, useMutation } from '../requests';
import { chatRequest } from './model';

type Settings = { about: string; style: string; checks: boolean; every_minutes: number; checklist: string; available?: boolean };
const intervals: [number, string][] = [[30, 'Every 30 minutes'], [60, 'Every hour'], [120, 'Every 2 hours'], [240, 'Every 4 hours'], [480, 'Every 8 hours'], [1440, 'Once a day']];

/** What your assistant knows about you, and its regular checks (schema 129). */
export function AssistantSettings({ name, close }: { name: string; close: () => void }) {
  const [form, setForm] = useState<Settings>();
  const [error, setError] = useState('');
  const save = useMutation();
  useEffect(() => {
    const abort = new AbortController();
    chatRequest<Settings>('/assistant/settings', undefined, abort.signal)
      .then(data => { if (!abort.signal.aborted) setForm(data); })
      .catch(e => { if (!abort.signal.aborted) setError(errorMessage(e)); });
    return () => abort.abort();
  }, []);
  const set = (change: Partial<Settings>) => setForm(previous => previous && { ...previous, ...change });
  const submit = () => void save.run(async () => {
    if (!form) return;
    await chatRequest('/assistant/settings', { about: form.about, style: form.style, checks: form.checks, every_minutes: form.every_minutes, checklist: form.checklist });
    close();
  });
  const unavailable = form?.available === false;
  return <Dialog title="Assistant settings" busy={save.busy} status={save.status || error} close={close}
    footer={<><Button onClick={close} disabled={save.busy}>Cancel</Button><Button primary disabled={!form || unavailable || save.busy} onClick={submit}>{save.busy ? 'Saving…' : 'Save'}</Button></>}>
    <div className="chat-threads">
      {!form && !error && <p className="chat-help" role="status">Loading…</p>}
      {unavailable && <p className="chat-help">This is not available in this workspace yet.</p>}
      {form && !unavailable && <>
        <label>What {name} knows about you
          <textarea value={form.about} maxLength={6000} onChange={event => set({ about: event.target.value })} placeholder="Your role, what matters to you this month, who you work with…" />
          <small className="chat-help">{name} adds a line here when it learns something lasting about you. Change or remove anything.</small></label>
        <label>How you write
          <textarea value={form.style} maxLength={3000} onChange={event => set({ style: event.target.value })} placeholder="Short and friendly. No exclamation marks. Signs off with first name…" />
          <small className="chat-help">Used when {name} drafts a reply or answers for you in a thread.</small></label>
        <label className="ui-check"><input type="checkbox" checked={form.checks} onChange={event => set({ checks: event.target.checked })} />
          <span>Check for me regularly<small className="chat-help">{name} looks at new direct messages and mentions for you and tells you what needs you, in your chat with it. When there is nothing new, it stays quiet.</small></span></label>
        {form.checks && <>
          <label>How often<select value={form.every_minutes} onChange={event => set({ every_minutes: Number(event.target.value) })}>
            {intervals.map(([minutes, label]) => <option key={minutes} value={minutes}>{label}</option>)}</select></label>
          <label>Also check every time (optional)
            <textarea value={form.checklist} maxLength={3000} onChange={event => set({ checklist: event.target.value })} placeholder="For example: are any of my open orders late?" /></label>
        </>}
      </>}
    </div>
  </Dialog>;
}

/** In the header of your assistant's own chat: opens its settings. */
export function AssistantSettingsButton({ name }: { name: string }) {
  const [open, setOpen] = useState(false);
  return <>
    <Button className="ui-icon chat-assistant-settings" aria-label="Assistant settings" title={`Assistant settings: what ${name} knows about you, how you write and regular checks`} onClick={() => setOpen(true)}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true"><path d="M4 7h9M17 7h3M4 17h3M11 17h9" /><circle cx="15" cy="7" r="2" /><circle cx="9" cy="17" r="2" /></svg>
    </Button>
    {open && <AssistantSettings name={name} close={() => setOpen(false)} />}
  </>;
}
