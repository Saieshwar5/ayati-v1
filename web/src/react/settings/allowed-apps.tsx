import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { dayName } from '../apps/apps-model';
import '../apps/catalog.css';
import { decideAppRequest, loadAllowedApps, loadAppRequests, saveAllowance, type AllowedApp, type AppRequest, type WaitingSystem } from '../apps/allowed-apps-model';
import { changeSystem } from '../agents/systems/model';
import { removeAddedApp } from '../apps/added-apps-model';
import { AddApps, type Reviewing } from './add-apps';
import { AppLetter } from '../apps/app-marks';
import { ConnectionRows, hasProblem, type Connections } from './app-connections';

type Asking = { title: string; help: string; action: string; run: (note: string) => Promise<string> };

type Filter = 'all' | 'allowed' | 'undecided' | 'blocked';
const words: Record<Exclude<Filter, 'all'>, string> = { allowed: 'Allowed', undecided: 'Ask first', blocked: 'Off' };
const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'}`;

const facts = (app: AllowedApp, connected: number) => [
  app.allowance === 'blocked' ? 'Nobody can use it' : connected ? plural(connected, 'connection') : app.allowance === 'undecided' ? 'Members ask an admin first' : 'Not connected yet',
  app.allowance === 'blocked' && app.note && `“${app.note}”`,
  app.added && `Added from the library · ${plural(app.added.actions.length, 'action')}`,
  app.decided && `${dayName(app.decided.at)} by ${app.decided.by_name}`,
].filter(Boolean).join(' · ');

/** Settings › Apps for admins (schema 134): what waits for an answer, then every app
 * Cuckoo offers with one choice (Allowed, Ask first until decided, Off) and its
 * connections under it. Members add allowed apps to their agents and ask for the rest.
 * `connections` are the workspace's connections, grouped here under their app;
 * `changed` tells their owner that some may have ended. */
export function AllowedApps({ changed, connections }: { changed?: () => void; connections?: Connections }) {
  const apps = useResource(loadAllowedApps);
  const requests = useResource(loadAppRequests);
  const mutation = useMutation();
  const [asking, ask] = useState<Asking>();
  // The Add apps dialog: searching the library, or changing one added app.
  const [adding, add] = useState<{ editing?: Reviewing }>();
  const [filter, setFilter] = useState<Filter>('all');
  // Apps whose connections are shown; one with a connection in trouble starts open.
  const [toggled, setToggled] = useState<Record<string, boolean>>({});
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
  const error = apps.error || requests.error || connections?.error;
  const list = apps.data?.apps ?? [];
  const under = (app: AllowedApp) => connections?.apps?.filter(item => item.server === app.id) ?? [];
  // MCP servers and anything else connected that is not one of the apps above.
  const others = connections?.apps?.filter(item => !list.some(app => app.id === item.server)) ?? [];
  const shown = list.filter(app => filter === 'all' || app.allowance === filter);
  const decide = (app: AllowedApp, choice: string) => { if (choice === 'allowed') allow(app); else if (choice === 'blocked') switchOff(app); };
  return <section className="settings-apps" aria-label="Apps">
    <header className="settings-section-heading"><div><h2>Apps</h2>
      <p>Which apps agents may use, and where each one is connected.</p></div>
      {apps.data?.library && <Button primary disabled={mutation.busy} onClick={() => add({})}>Add apps</Button>}</header>
    {error && <p role="alert">{error} <Button onClick={() => void refresh().catch(() => {})}>Retry</Button></p>}
    {!apps.data && !error && <p role="status">Loading apps…</p>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status || connections?.status}</p>
    {waiting.length + systems.length > 0 && <section className="settings-waiting-panel" aria-label="Needs your answer"><h3>Needs your answer · {waiting.length + systems.length}</h3>
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
      </li>)}</ul></section>}
    {list.length > 0 && <div className="settings-filters" role="group" aria-label="Show">{(['all', 'allowed', 'undecided', 'blocked'] as Filter[]).map(id =>
      <Button key={id} aria-pressed={filter === id} primary={filter === id} onClick={() => setFilter(id)}>
        {id === 'all' ? 'All' : words[id]} · {id === 'all' ? list.length : list.filter(app => app.allowance === id).length}</Button>)}</div>}
    {apps.data && !list.length && <p className="ui-empty">Cuckoo offers no apps here yet.</p>}
    {list.length > 0 && !shown.length && <p className="ui-empty">No apps here.</p>}
    {shown.length > 0 && <ul className="settings-rows settings-app-list" aria-label="Apps">{shown.map(app => {
      const connected = under(app);
      const more = connected.length > 0 || Boolean(app.added);
      const open = more && (toggled[app.id] ?? connected.some(hasProblem));
      return <li key={app.id} className="settings-app">
        <div className="settings-row">
          <AppLetter id={app.id} name={app.name} size="small" />
          <span><strong>{app.name}</strong><small>{facts(app, connections ? connected.length : app.connections)}</small></span>
          <span className="settings-row-actions">
            <select aria-label={`${app.name}: who may use it`} disabled={mutation.busy} value={app.allowance} onChange={event => decide(app, event.target.value)}>
              {app.allowance === 'undecided' && <option value="undecided">Ask first</option>}
              <option value="allowed">Allowed</option><option value="blocked">Off</option></select>
            {more && <Button className="ui-icon settings-app-toggle" aria-label={`${connected.length ? 'Connections' : 'Details'} of ${app.name}`} aria-expanded={open}
              onClick={() => setToggled(old => ({ ...old, [app.id]: !open }))}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg></Button>}
          </span>
        </div>
        {open && <div className="settings-app-more">
          {connections && connected.length > 0 && <ConnectionRows apps={connected} connections={connections} />}
          {app.added && <p className="settings-app-added"><span>From the library · {plural(app.added.actions.length, 'action')}</span>
            {apps.data?.library && <Button disabled={mutation.busy} aria-label={`Change the actions of ${app.name}`} onClick={() => changeActions(app)}>Change actions</Button>}
            <Button disabled={mutation.busy} aria-label={`Remove ${app.name}`} onClick={() => removeApp(app)}>Remove app</Button></p>}
        </div>}
      </li>;
    })}</ul>}
    {list.length > 0 && <p className="ui-help settings-apps-note">Allowed: people add it to their own agents. Ask first: they ask an admin. Off: nobody can use it, and its connections end.</p>}
    {connections && others.length > 0 && <section className="settings-other-connections" aria-label="Other connections"><h3>MCP servers and other connections</h3>
      <ConnectionRows apps={others} connections={connections} named /></section>}
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
