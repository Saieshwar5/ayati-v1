import { useState, type Ref } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { AgentFace } from '../faces';
import { workspaceUrl } from '../../workspace-context';
import { approveSystem, connectAccount, type WorkspaceConnectionsData } from '../settings/workspace-connections-model';
import { OutcomeDialog } from '../approvals/outcome-dialog';
import { agoText, appState, dayName, rememberConnecting, type WorkspaceApp } from './apps-model';
import { AppLetter, PauseIcon, RefreshIcon, stateWords } from './app-marks';
import { ServiceRules } from './service-rules';
import './service-panel.css';

const together = (names: string[]) => names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;

/** The one place to manage a connected service (designs/connector-screens 1): its
 * health, each action's rule, who uses it, the account, Reconnect and Disconnect.
 * Members see the same panel read-only. Admin data comes from `/control/connections`. */
export function ServicePanel({ app, canManage, control, controlError, retryControl, changed, disconnected, rulesHeading }: {
  app: WorkspaceApp; canManage: boolean; control?: WorkspaceConnectionsData; controlError?: string; retryControl: () => void;
  changed: () => Promise<void>; disconnected: (message: string) => void; rulesHeading?: Ref<HTMLHeadingElement>;
}) {
  const mutation = useMutation();
  const [checking, setChecking] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const state = appState(app);
  const safety = canManage ? control?.safety.find(s => s.connection === app.id) : undefined;
  const connection = control?.connections.find(c => c.id === app.id);
  const reconnect = () => void mutation.run(async () => {
    if (!connection) return;
    const result = await connectAccount({ id: connection.id, server: app.server, enabled: true, revision: connection.revision });
    if (result.redirect_url) { rememberConnecting(app.server); setLeaving(true); location.assign(result.redirect_url); return; }
    await changed();
    mutation.setStatus(`${app.name} is connected again.`);
  });
  return <aside className="app-panel service-panel" aria-label={app.name}>
    <div className="service-head"><AppLetter id={app.server} name={app.name} size="big" />
      <div><h2>{app.name}</h2><p className="app-line">{app.account ? `Connected as ${app.account}` : `Service · ${state === 'ok' ? 'Connected' : stateWords[state]}`}</p></div></div>
    {state === 'writes_paused' && <div className="app-callout service-callout">
      <p><PauseIcon /><span><strong>Writes are paused.</strong> {app.name} didn’t confirm one earlier action, so Cuckoo holds every write until someone checks it. Reading still works.</span></p>
      {app.paused_action && <Button onClick={() => setChecking(true)}>Check what happened</Button>}</div>}
    {state === 'reconnect' && <p className="app-callout">The sign-in expired. Agents can’t use {app.name} until {canManage ? 'you reconnect it' : 'an admin reconnects it'}.</p>}
    {state === 'paused' && <p className="app-callout quiet">Cuckoo paused {app.name} for a short while. Nothing is sent to it until it resumes.</p>}
    {canManage && controlError && <p role="alert" className="service-error">{controlError} <Button onClick={retryControl}>Retry</Button></p>}
    <ServiceRules key={app.id} app={app} safety={safety} changed={changed} heading={rulesHeading} />
    <section className="service-section" aria-label="Used by"><h3>Used by</h3>
      <div className="service-used">
        {app.agents.length ? <><span className="app-faces">{app.agents.slice(0, 4).map(agent => <AgentFace key={agent.id} agent={agent} className="app-face" />)}</span>
          <span>{together(app.agents.map(agent => agent.name))}</span></>
          : <span className="app-line">No agent uses it yet. An agent’s owner switches it on from the agent’s Tools tab.</span>}
        {app.rules > 0 && <a href={workspaceUrl('/?view=settings&section=company-rules')}>{app.rules} company rule{app.rules === 1 ? '' : 's'}</a>}
      </div></section>
    <AccountFacts app={app} admin={canManage} />
    {canManage && <footer className="service-foot">
      <Button primary={state === 'reconnect'} disabled={mutation.busy || leaving || !connection} onClick={reconnect}><RefreshIcon />{leaving ? 'Opening sign-in…' : 'Reconnect'}</Button>
      <button type="button" className="service-disconnect" disabled={mutation.busy || leaving || !control} onClick={() => setConfirming(true)}>Disconnect {app.name}…</button>
    </footer>}
    {mutation.status && <p role="alert">{mutation.status}</p>}
    {checking && app.paused_action && <OutcomeDialog id={app.paused_action} close={() => { setChecking(false); void changed().catch(() => {}); }} />}
    {confirming && <DisconnectDialog app={app} revision={control?.systems.find(s => s.server === app.server)?.revision ?? 0}
      close={() => setConfirming(false)} disconnected={disconnected} changed={changed} />}
  </aside>;
}

/** Who connected the account and when, when agents last used it and whether its
 * sign-in works. The connection service is named for admins only. */
function AccountFacts({ app, admin }: { app: WorkspaceApp; admin: boolean }) {
  const state = appState(app);
  const signIn = state === 'reconnect' ? 'Expired' : state === 'paused' ? 'Paused by Cuckoo' : 'Works';
  return <section className="service-section" aria-label="Account"><h3>Account</h3>
    <dl className="service-facts">
      {app.connected && <div><dt>Connected by</dt><dd>{app.connected.by_name}, {dayName(app.connected.at)}</dd></div>}
      {app.last_used !== undefined && <div><dt>Last used</dt><dd>{app.last_used ? `${agoText(app.last_used.at)}, by ${app.last_used.agent_name}` : 'Not used yet'}</dd></div>}
      <div><dt>Sign-in</dt><dd>{signIn}{admin && app.via && <span className="service-via"> · through {app.via}</span>}</dd></div>
    </dl></section>;
}

/** Disconnecting removes the app's approval, which also turns off its account and
 * releases it at the provider, in one request. Connecting again starts with safe rules. */
function DisconnectDialog({ app, revision, close, disconnected, changed }: {
  app: WorkspaceApp; revision: number; close: () => void; disconnected: (message: string) => void; changed: () => Promise<void>;
}) {
  const mutation = useMutation();
  const confirm = () => void mutation.run(async () => {
    let result;
    // A refusal (often a stale revision) reloads, so confirming again uses current data.
    try { result = await approveSystem({ server: app.server, approved: false, revision }); }
    catch (failure) { await changed().catch(() => {}); throw failure; }
    close();
    disconnected(result.provider_revoked === false
      ? `${app.name} is disconnected in Cuckoo. ${app.name} didn’t confirm removing Cuckoo’s access, so remove it there too.`
      : `${app.name} is disconnected. Agents can no longer use it.`);
  });
  return <Dialog title={`Disconnect ${app.name}?`} close={close} busy={mutation.busy} status={mutation.status}
    footer={<><Button disabled={mutation.busy} onClick={close}>Cancel</Button>
      <Button primary className="service-danger" disabled={mutation.busy} onClick={confirm}>Disconnect {app.name}</Button></>}>
    <p className="service-confirm">Agents lose access to {app.name} at once{app.agents.length ? `, including ${together(app.agents.map(agent => agent.name))}` : ''}.
      If you connect it again, its rules start over from Cuckoo’s safe defaults.</p>
  </Dialog>;
}
