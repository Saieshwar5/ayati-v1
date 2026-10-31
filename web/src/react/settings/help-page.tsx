import { useRef, useState } from 'react';
import { currentWorkspace, workspaceUrl } from '../../workspace-context';
import { chatRequest } from '../chat/model';
import { Button } from '../ui';
import { useMutation } from '../requests';

const topics: [string, string][] = [
  ['agent', 'An agent did something unexpected'], ['chat', 'Chat or notifications'], ['connections', 'Connections and approvals'],
  ['library', 'Library and files'], ['billing', 'Account or billing'], ['other', 'Something else'],
];
const guides: [string, string][] = [
  ['Invite your team', '/?view=settings&section=people'], ['Create your first agent', '/?view=agents'],
  ['Connect your business tools', '/?view=settings&section=connections'], ['See what your agents did', '/?view=activity&tab=history'],
];

/** Help (designs/settings 10): "Report a problem" goes to the Cuckoo team, and
 * short guides for the first days. Technical details never include messages. */
export function HelpPage() {
  const [topic, setTopic] = useState(topics[0][0]);
  const [body, setBody] = useState('');
  const [technical, setTechnical] = useState(true);
  // One id per report, so a retry after a lost response stores it once.
  const id = useRef(crypto.randomUUID());
  const mutation = useMutation();
  const send = () => void mutation.run(async () => {
    await chatRequest('/problem-reports', { id: id.current, topic, body,
      technical: technical ? { page: new URL(window.location.href).searchParams.get('view') ?? 'home', browser: navigator.userAgent.slice(0, 300),
        language: navigator.language, screen: `${window.innerWidth}×${window.innerHeight}`, workspace: currentWorkspace()?.name ?? '' } : undefined });
    id.current = crypto.randomUUID(); setBody('');
    mutation.setStatus('Report sent. The Cuckoo team replies by email.');
  });
  return <section className="settings-help"><h2>Help</h2><p>Guides, and a direct line to the Cuckoo team.</p>
    <div className="settings-help-grid">
      <form className="ui-form settings-help-card" aria-label="Report a problem" onSubmit={event => { event.preventDefault(); send(); }}>
        <h3>Report a problem</h3><p>Goes to the Cuckoo team. We reply by email.</p>
        <fieldset className="ui-fieldset" disabled={mutation.busy}>
          <label className="ui-field">What is it about?<select value={topic} onChange={event => setTopic(event.target.value)}>
            {topics.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
          <label className="ui-field">What happened?<textarea required maxLength={4000} rows={4} value={body} onChange={event => setBody(event.target.value)} /></label>
          <label className="settings-check"><input type="checkbox" checked={technical} onChange={event => setTechnical(event.target.checked)} />Include technical details (no message contents)</label>
          <Button type="submit" primary disabled={mutation.busy || !body.trim()}>{mutation.busy ? 'Sending…' : 'Send report'}</Button>
        </fieldset>
        <p role="status" className="ui-status">{mutation.status}</p>
      </form>
      <nav className="settings-help-card" aria-label="Get started"><h3>Get started</h3>
        <ul className="settings-guides">{guides.map(([label, href]) => <li key={label}><a href={workspaceUrl(href)}>{label}<span aria-hidden="true">›</span></a></li>)}</ul>
      </nav>
    </div>
  </section>;
}
