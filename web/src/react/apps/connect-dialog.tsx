import { useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { modes, type Mode } from '../agents/control-model';
import { loadConnections, meanings, type Meaning } from '../settings/workspace-connections-model';
import { connectApp, guessMeaning, rememberConnecting, rulesSummary, safeDefault, type CatalogApp, type WorkspaceApp } from './apps-model';
import { CheckIcon } from './app-marks';
import './catalog.css';

type Row = { key: string; title: string; note: string; mode: Mode };

/** Connect (designs/connector-screens 3): approve, apply Cuckoo's safe rules and start
 * sign-in in one request, with the starting rules shown first. A retry of the same app
 * reuses the request id, so the server answers it once. `keepPage` opens sign-in in a
 * new tab, so a draft on this page (a new agent) is kept. */
export function ConnectDialog({ app, keepPage = false, close, connected }: {
  app: Pick<CatalogApp, 'id' | 'name' | 'kinds'>; keepPage?: boolean; close: () => void; connected: (connection?: string) => void;
}) {
  const control = useResource(loadConnections);
  const mutation = useMutation();
  const request = useRef<string>(undefined);
  const [signIn, setSignIn] = useState<string>();
  const [leaving, setLeaving] = useState(false);
  const server = control.data?.servers.find(s => s.id === app.id);
  // The app's own actions when this Cuckoo lists them, else one row per kind.
  const rows: Row[] = server ? server.operations.map(operation => { const meaning = guessMeaning(operation); return { key: operation.id, title: operation.title, note: meanings[meaning], mode: safeDefault(meaning) }; })
    : app.kinds.map((kind: Meaning) => ({ key: kind, title: meanings[kind], note: 'Every action of this kind', mode: safeDefault(kind) }));
  const start = () => void mutation.run(async () => {
    request.current ??= crypto.randomUUID();
    const result = await connectApp(app.id, request.current);
    if (result.redirect_url && keepPage) { setSignIn(result.redirect_url); return; }
    if (result.redirect_url) { rememberConnecting(app.id); setLeaving(true); location.assign(result.redirect_url); return; }
    connected(result.connection);
  });
  const busy = mutation.busy || leaving;
  return <Dialog title={`Connect ${app.name}`} close={close} busy={busy} status={mutation.status}
    footer={signIn ? <><Button onClick={close}>Cancel</Button><Button primary onClick={() => connected()}>Done</Button></> : <>
      <span className="ui-help connect-hint">Opens {app.name} {keepPage ? 'in a new tab' : 'in this tab'}</span>
      <Button disabled={busy} onClick={close}>Cancel</Button>
      <Button primary disabled={busy} onClick={start}>{leaving ? `Opening ${app.name}…` : `Continue to ${app.name} →`}</Button></>}>
    {signIn ? <div className="connect">
      <p>Sign in on {app.name}’s own page, in a new tab. Use the account agents should work in. This page stays as it is.</p>
      <a className="ui-button primary" href={signIn} target="_blank" rel="noopener noreferrer">Open {app.name} sign-in</a>
      <p className="ui-help">When you have signed in, come back here and choose Done.</p>
    </div> : <div className="connect">
      <div><h3>What agents could do</h3><p className="ui-help">These start with safe rules. You can change any of them after connecting.</p></div>
      {!control.data && !control.error && <p role="status">Loading {app.name}’s actions…</p>}
      {(control.data || control.error) && <ul className="connect-rules" aria-label="Starting rules">{rows.map(row => <li key={row.key}>
        <span>{row.title}<small>{row.note}</small></span><RulePreview mode={row.mode} />
      </li>)}</ul>}
      <div><h3>Sign in</h3><p>Next you’ll sign in on {app.name}’s own page. Use the account agents should work in. A shared company account is best, so it keeps working when people change roles.</p></div>
      <p className="connect-notice"><LockMark /><span>No agent can use {app.name} until its owner switches it on. Cuckoo checks every action against these rules before it runs.</span></p>
    </div>}
  </Dialog>;
}

/** Allow / Ask first / Block as a picture of the starting rule; it is not a control. */
function RulePreview({ mode }: { mode: Mode }) {
  return <span className="rule-preview" role="img" aria-label={`Starts as ${modes[mode]}`}>
    {(Object.keys(modes) as Mode[]).map(option => <span key={option} className={`${option}${option === mode ? ' chosen' : ''}`} aria-hidden="true">{modes[option]}</span>)}
  </span>;
}

/** After sign-in: the account that connected, its starting rules and the next steps. */
export function ConnectedDialog({ app, close, chooseAgents, changeRules }: { app?: WorkspaceApp; close: () => void; chooseAgents: () => void; changeRules: () => void }) {
  return <Dialog title={app ? `${app.name} is connected` : 'Account connected'} close={close}>
    <div className="connect">
      <p className="connect-notice ok" role="status"><CheckIcon /><span>{app?.account ? <>Connected as <strong>{app.account}</strong>. </> : 'Connected. '}
        {app ? rulesSummary(app.actions) : 'It starts with Cuckoo’s safe rules.'}</span></p>
      <p>Next, switch it on for the agents that need it. Their owners can make the rules stricter, never looser.</p>
      <div className="connect-next">
        <Button primary onClick={chooseAgents}>Choose agents</Button>
        <Button disabled={!app} onClick={changeRules}>Change rules</Button>
        <Button className="quiet" onClick={close}>Done</Button>
      </div>
    </div>
  </Dialog>;
}

function LockMark() {
  return <svg className="mark-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
    <path d="M4 7.5h8v6H4zM5.5 7.5V5.5a2.5 2.5 0 0 1 5 0v2" /></svg>;
}
