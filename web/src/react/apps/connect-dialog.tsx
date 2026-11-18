import { useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { modes, type Mode } from '../agents/control-model';
import { meanings } from '../settings/workspace-connections-model';
import { connectApp, safeDefault, type CatalogApp } from './apps-model';
import './catalog.css';

/** Connect an app for one agent (schema 132): the starting rules are shown, then
 * one request approves the app, applies Cuckoo's safe rules and opens the app's
 * own sign-in page in this tab. The sign-in returns to the agent's Connections
 * tab. A retry of the same app reuses the request id, so the server answers once. */
export function ConnectDialog({ app, agent, close, connected }: {
  app: Pick<CatalogApp, 'id' | 'name' | 'kinds' | 'allowance'>; agent: { id: string; name: string }; close: () => void; connected: (connection?: string) => void;
}) {
  const mutation = useMutation();
  const request = useRef<string>(undefined);
  const [leaving, setLeaving] = useState(false);
  const start = () => void mutation.run(async () => {
    request.current ??= crypto.randomUUID();
    const result = await connectApp(app.id, request.current, agent.id);
    if (result.redirect_url) { setLeaving(true); location.assign(result.redirect_url); return; }
    connected(result.connection);
  });
  const busy = mutation.busy || leaving;
  return <Dialog title={`Connect ${app.name}`} close={close} busy={busy} status={mutation.status}
    footer={<>
      <span className="ui-help connect-hint">Opens {app.name} in this tab</span>
      <Button disabled={busy} onClick={close}>Cancel</Button>
      <Button primary disabled={busy} onClick={start}>{leaving ? `Opening ${app.name}…` : `Continue to ${app.name} →`}</Button></>}>
    <div className="connect">
      <div><h3>What {agent.name} could do</h3><p className="ui-help">These start with safe rules. You can change any of them after connecting.</p></div>
      <ul className="connect-rules" aria-label="Starting rules">{app.kinds.map(kind => <li key={kind}>
        <span>{meanings[kind]}<small>Every action of this kind</small></span><RulePreview mode={safeDefault(kind)} />
      </li>)}</ul>
      <div><h3>Sign in</h3><p>Next you’ll sign in on {app.name}’s own page. Use the account {agent.name} should work in.</p></div>
      {app.allowance && app.allowance !== 'allowed' && <p className="connect-notice">Adding {app.name} also allows it for this workspace: other people can then add it to their own agents.</p>}
      <p className="connect-notice"><LockMark /><span>Only {agent.name} can use this {app.name} account. Cuckoo checks every action against these rules before it runs.</span></p>
    </div>
  </Dialog>;
}

/** Allow / Ask first / Block as a picture of the starting rule; it is not a control. */
function RulePreview({ mode }: { mode: Mode }) {
  return <span className="rule-preview" role="img" aria-label={`Starts as ${modes[mode]}`}>
    {(Object.keys(modes) as Mode[]).map(option => <span key={option} className={`${option}${option === mode ? ' chosen' : ''}`} aria-hidden="true">{modes[option]}</span>)}
  </span>;
}

function LockMark() {
  return <svg className="mark-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
    <path d="M4 7.5h8v6H4zM5.5 7.5V5.5a2.5 2.5 0 0 1 5 0v2" /></svg>;
}
