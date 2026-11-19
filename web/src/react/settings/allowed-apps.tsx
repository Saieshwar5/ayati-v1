import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { dayName } from '../apps/apps-model';
import '../apps/catalog.css';
import { decideAppRequest, loadAllowedApps, loadAppRequests, saveAllowance, type AllowedApp, type AppRequest, type WaitingSystem } from '../apps/allowed-apps-model';
import { changeSystem } from '../agents/systems/model';
import { removeAddedApp } from '../apps/added-apps-model';
import { AddApps, type Reviewing } from './add-apps';

type Asking = { title: string; help: string; action: string; run: (note: string) => Promise<string> };

const facts = (app: AllowedApp) => [
  app.added && `Added from the library · ${app.added.actions.length} action${app.added.actions.length === 1 ? '' : 's'}`,
  app.allowance === 'allowed' ? 'Allowed' : app.allowance === 'blocked' ? 'Switched off' : 'Not decided: members ask first',
  app.allowance === 'blocked' && app.note,
  app.decided && `${dayName(app.decided.at)} by ${app.decided.by_name}`,
  app.connections > 0 && `${app.connections} connection${app.connections === 1 ? '' : 's'}`,
].filter(Boolean).join(' · ');

/** Settings › Connections for admins (schema 134): the requests waiting for an
 * answer, and every app Cuckoo offers with the admins' decision. Members add
 * allowed apps to their agents and ask for the rest. `changed` tells the list
 * of connections below that some may have ended. */
export function AllowedApps({ changed }: { changed?: () => void }) {
  const apps = useResource(loadAllowedApps);
  const requests = useResource(loadAppRequests);
  const mutation = useMutation();
  const [asking, ask] = useState<Asking>();
  // The Add more apps dialog: searching the library, or changing one added app.
  const [adding, add] = useState<{ editing?: Reviewing }>();
  const refresh = async () => { await Promise.all([apps.refresh(), requests.refresh()]); changed?.(); };
  const change = (work: () => Promise<string>) => void mutation.run(async () => { const done = await work(); await refresh(); mutation.setStatus(done); });
  const allow = (app: AllowedApp) => change(async () => {
    await saveAllowance({ app: app.id, allowed: true, note: '', revision: app.revision });
    return `${app.name} is allowed.`;
  });
  const switchOff = (app: AllowedApp) => ask({
    title: `Switch off ${app.name}`, action: 'Switch off',
    help: app.connections ? `${app.connections} connection${app.connections === 1 ? '' : 's'} of ${app.name} end at once, and the agents using them stop. Allowing it later means signing in again.`
      : `Nobody can add ${app.name} to an agent until you allow it again.`,
    run: async note => { await saveAllowance({ app: app.id, allowed: false, note, revision: app.revision }); return `${app.name} is switched off.`; },
  });
  const decline = (request: AppRequest) => ask({
    title: `Decline ${request.name}`, action: 'Decline', help: `${request.requester} sees your answer on Home. The app stays undecided, so it can be asked for again.`,
    run: async note => { await decideAppRequest(request.id, 'decline', note); return `Declined ${request.requester}’s request.`; },
  });
  const approve = (request: AppRequest) => change(async () => {
    await decideAppRequest(request.id, 'approve', '');
    return `${request.name} is allowed. ${request.requester} can add it now.`;
  });
  const answerSystem = (system: WaitingSystem, answer: 'allow' | 'decline') => {
    if (answer === 'decline' && !confirm(`Decline ${system.name}? It is removed from ${system.agent_name}.`)) return;
    change(async () => {
      await changeSystem(system.agent, system.id, system.revision, answer);
      return answer === 'allow' ? `${system.name} is allowed. ${system.agent_name} can use it now.` : `${system.name} was declined and removed.`;
    });
  };
  const changeActions = (app: AllowedApp) => app.added && add({ editing: { app: app.added.toolkit, name: app.name, added: app.added, connections: app.connections } });
  const removeApp = (app: AllowedApp) => {
    const ends = app.connections ? ` ${app.connections} connection${app.connections === 1 ? '' : 's'} of it end at once.` : '';
    if (!app.added || !confirm(`Remove ${app.name} from this workspace?${ends} You can add it again later.`)) return;
    const revision = app.added.revision;
    change(async () => { await removeAddedApp(app.id, revision); return `${app.name} was removed.`; });
  };
  const systems = apps.data?.systems ?? [];
  // Requests for catalog apps; other kinds have no screen.
  const waiting = requests.data?.requests.filter(request => request.connector) ?? [];
  const error = apps.error || requests.error;
  return <section className="settings-section" aria-label="Allowed apps">
    <h2>Allowed apps</h2>
    <p className="ui-help">People add these apps to their own agents. An app you haven’t decided about must be asked for, and one you add to an agent yourself becomes allowed. An MCP server a member connects also waits for your yes.</p>
    {apps.data?.library && <p><Button primary disabled={mutation.busy} onClick={() => add({})}>Add more apps</Button></p>}
    {error && <p role="alert">{error} <Button onClick={() => void refresh().catch(() => {})}>Retry</Button></p>}
    {!apps.data && !error && <p role="status">Loading apps…</p>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    {waiting.length + systems.length > 0 && <><h3 className="settings-subheading">Waiting for your answer</h3>
      <ul className="settings-rows" aria-label="App requests">{systems.map(system => <li key={system.id} className="settings-row">
        <span><strong>{system.requester ?? 'Someone'} connected the MCP server {system.name}</strong>
          <small>{[`for ${system.agent_name}`, system.url, `${system.tools} tool${system.tools === 1 ? '' : 's'}`].join(' · ')}</small></span>
        <span className="settings-row-actions">
          <Button primary disabled={mutation.busy} aria-label={`Allow the MCP server ${system.name}`} onClick={() => answerSystem(system, 'allow')}>Allow</Button>
          <Button disabled={mutation.busy} aria-label={`Decline the MCP server ${system.name}`} onClick={() => answerSystem(system, 'decline')}>Decline</Button></span>
      </li>)}{waiting.map(request => <li key={request.id} className="settings-row">
        <span><strong>{request.requester} asks for {request.name}</strong>
          <small>{[request.agent_name && `for ${request.agent_name}`, request.reason && `“${request.reason}”`, `asked ${dayName(request.created)}`].filter(Boolean).join(' · ')}</small></span>
        <span className="settings-row-actions">
          <Button primary disabled={mutation.busy} aria-label={`Allow ${request.name} for ${request.requester}`} onClick={() => approve(request)}>Allow</Button>
          <Button disabled={mutation.busy} aria-label={`Decline ${request.name} for ${request.requester}`} onClick={() => decline(request)}>Decline</Button></span>
      </li>)}</ul></>}
    {apps.data && !apps.data.apps.length && <p className="ui-empty">Cuckoo offers no apps here yet.</p>}
    {apps.data && apps.data.apps.length > 0 && <ul className="settings-rows" aria-label="Apps">{apps.data.apps.map(app => <li key={app.id} className="settings-row">
      <span><strong>{app.name}</strong><small>{facts(app)}</small></span>
      <span className="settings-row-actions">
        {app.allowance !== 'allowed' && <Button disabled={mutation.busy} aria-label={`Allow ${app.name}`} onClick={() => allow(app)}>Allow</Button>}
        {app.allowance !== 'blocked' && <Button disabled={mutation.busy} aria-label={`Switch off ${app.name}`} onClick={() => switchOff(app)}>Switch off</Button>}
        {app.added && apps.data?.library && <Button disabled={mutation.busy} aria-label={`Change the actions of ${app.name}`} onClick={() => changeActions(app)}>Change actions</Button>}
        {app.added && <Button disabled={mutation.busy} aria-label={`Remove ${app.name}`} onClick={() => removeApp(app)}>Remove</Button>}</span>
    </li>)}</ul>}
    {adding && <AddApps editing={adding.editing} close={() => add(undefined)} done={status => { add(undefined); void refresh().then(() => mutation.setStatus(status)).catch(() => {}); }} />}
    {asking && <NoteDialog asking={asking} close={() => ask(undefined)} done={status => { ask(undefined); void refresh().then(() => mutation.setStatus(status)).catch(() => {}); }} />}
  </section>;
}

/** A "no" with a few words for the people it affects. */
function NoteDialog({ asking, close, done }: { asking: Asking; close: () => void; done: (status: string) => void }) {
  const mutation = useMutation();
  const [note, setNote] = useState('');
  return <Dialog title={asking.title} close={close} busy={mutation.busy} status={mutation.status}
    footer={<><Button disabled={mutation.busy} onClick={close}>Cancel</Button>
      <Button primary disabled={mutation.busy} onClick={() => void mutation.run(async () => done(await asking.run(note.trim())))}>{mutation.busy ? 'Saving…' : asking.action}</Button></>}>
    <div className="connect">
      <p>{asking.help}</p>
      <label className="ui-field"><span>A note people will see (optional)</span>
        <textarea className="request-reason" maxLength={300} value={note} onChange={event => setNote(event.target.value)} /></label>
    </div>
  </Dialog>;
}
