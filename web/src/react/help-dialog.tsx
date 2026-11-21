import { useId, useRef, useState } from 'react';
import { currentWorkspace, workspaceUrl } from '../workspace-context';
import { chatRequest } from './chat/model';
import { Button, Dialog } from './ui';
import { useMutation } from './requests';

const topics: [string, string][] = [
  ['agent', 'An agent did something unexpected'], ['chat', 'Chat or notifications'], ['connections', 'Apps and approvals'],
  ['library', 'Library and files'], ['billing', 'Account or billing'], ['other', 'Something else'],
];
const guides: [string, string, boolean][] = [
  ['Invite your team', '/?view=settings&section=people', true], ['Create your first agent', '/?view=agents', false],
  ['Connect your business tools', '/?view=agents', false], ['See what your agents did', '/?view=activity&tab=history', false],
];

/** Help and feedback, opened from the account menu (it was a Settings page until
 * 2026-11-21): "Report a problem" goes to the Cuckoo team, beside short guides for
 * the first days. Technical details never include messages. */
export function HelpDialog({ close }: { close: () => void }) {
  const [topic, setTopic] = useState(topics[0][0]);
  const [body, setBody] = useState('');
  const [technical, setTechnical] = useState(true);
  // One id per report, so a retry after a lost response stores it once.
  const id = useRef(crypto.randomUUID());
  const form = useId();
  const mutation = useMutation();
  const admin = currentWorkspace()?.role === 'Admin';
  const send = () => void mutation.run(async () => {
    await chatRequest('/problem-reports', { id: id.current, topic, body,
      technical: technical ? { page: new URL(window.location.href).searchParams.get('view') ?? 'home', browser: navigator.userAgent.slice(0, 300),
        language: navigator.language, screen: `${window.innerWidth}×${window.innerHeight}`, workspace: currentWorkspace()?.name ?? '' } : undefined });
    id.current = crypto.randomUUID(); setBody('');
    mutation.setStatus('Report sent. The Cuckoo team replies by email.');
  });
  return <Dialog title="Help and feedback" busy={mutation.busy} status={mutation.status} close={close}
    footer={<><Button onClick={close}>Close</Button>
      <Button type="submit" form={form} primary disabled={mutation.busy || !body.trim()}>{mutation.busy ? 'Sending…' : 'Send report'}</Button></>}>
    <div className="help-dialog">
      <form id={form} className="ui-form" aria-label="Report a problem" onSubmit={event => { event.preventDefault(); send(); }}>
        <h3>Report a problem</h3><p className="ui-help">Goes to the Cuckoo team. We reply by email.</p>
        <label className="ui-field">What is it about?<select value={topic} onChange={event => setTopic(event.target.value)}>
          {topics.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
        <label className="ui-field">What happened?<textarea required maxLength={4000} rows={4} value={body} onChange={event => setBody(event.target.value)} /></label>
        <label className="ui-check help-dialog-check"><input type="checkbox" checked={technical} onChange={event => setTechnical(event.target.checked)} />Include technical details (never message contents)</label>
      </form>
      <nav aria-label="Get started"><h3>Get started</h3>
        <ul className="help-dialog-guides">{guides.filter(guide => admin || !guide[2]).map(([label, href]) =>
          <li key={label}><a href={workspaceUrl(href)}>{label}<span aria-hidden="true">›</span></a></li>)}</ul>
      </nav>
    </div>
  </Dialog>;
}
