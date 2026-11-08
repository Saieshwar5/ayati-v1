import { useCallback, useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { useResource } from '../requests';
import { currentWorkspace, workspaceUrl } from '../../workspace-context';
import { navigate } from '../../navigation';
import { loadConnections } from '../settings/workspace-connections-model';
import { forgetConnecting, loadApps, needsYou, peekConnecting, toMs, type CatalogApp, type CompanySystem } from './apps-model';
import { RequestDialog, RequestsDrawer, SystemForm, SystemPanel } from './company-apps';
import { BuiltForm, BuiltPanel } from './built-apps';
import { AppRunner } from './built-app-runner';
import { AppCard, type Card } from './app-card';
import { PlusIcon } from './app-marks';
import { ServicePanel } from './service-panel';
import { CatalogDialog } from './catalog-dialog';
import { ConnectDialog, ConnectedDialog } from './connect-dialog';
import './apps.css';

type Filter = 'all' | 'built' | 'systems' | 'services';
/** Back from a provider's sign-in page (`connected=1` or `connect_error=1`), or connected at once. */
type Arrival = { result: 'connected' | 'failed'; app?: string };

/** Apps (apps-page plan, AP1–AP3; designs/connector-screens): the company's own
 * systems, apps built here and every connected service with what agents may do.
 * Everyone reads it and opens or requests apps; admins add and manage them, each
 * service in its own panel. */
export function AppsPage({ href }: { href: string }) {
  const url = new URL(href);
  const running = url.searchParams.get('run');
  return running ? <AppRunner id={running} preview={url.searchParams.get('preview') === '1'} /> : <AppsList url={url} />;
}

function arrivalFrom(url: URL): Arrival | undefined {
  const result = url.searchParams.get('connected') ? 'connected' : url.searchParams.get('connect_error') ? 'failed' : undefined;
  return result && { result, app: peekConnecting() };
}

function AppsList({ url }: { url: URL }) {
  const admin = currentWorkspace()?.role === 'Admin';
  const resource = useResource(loadApps);
  // Admins' rules, revisions and approvals load beside the apps, not after them.
  const loadControl = useCallback(() => admin ? loadConnections() : Promise.resolve(undefined), [admin]);
  const control = useResource(loadControl);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [needsOnly, setNeedsOnly] = useState(false);
  const [selected, setSelected] = useState<string>();
  // Old Connections links and "Ask an admin to reconnect" land here: show what needs attention.
  const [attention] = useState(() => url.searchParams.get('manage') === '1' || url.searchParams.get('section') === 'connections');
  const [arrival, setArrival] = useState(() => arrivalFrom(url));
  const [notice, setNotice] = useState('');
  const [adding, setAdding] = useState(false);
  const [connecting, setConnecting] = useState<CatalogApp>();
  const [editing, setEditing] = useState<CompanySystem | 'new'>();
  const [building, setBuilding] = useState(false);
  const [suggesting, setSuggesting] = useState(false);
  const [deciding, setDeciding] = useState(false);
  const rulesHeading = useRef<HTMLHeadingElement>(null);
  const returned = url.searchParams.has('connected') || url.searchParams.has('connect_error');
  useEffect(() => {
    if (!returned) return;
    // Read once: a reload or Back does not announce the same sign-in again.
    forgetConnecting();
    const clean = new URL(window.location.href);
    clean.searchParams.delete('connected'); clean.searchParams.delete('connect_error');
    if (clean.href !== window.location.href) window.history.replaceState(window.history.state, '', clean);
  }, [returned]);
  const data = resource.data;
  const changed = () => Promise.all([resource.refresh(), control.refresh()]).then(() => undefined);
  const refresh = () => void changed().catch(() => {});
  const needs = data?.apps.filter(needsYou).length ?? 0;
  const matches = (name: string) => !query.trim() || name.toLowerCase().includes(query.trim().toLowerCase());
  const cards: Card[] = [
    ...(['all', 'built'].includes(filter) && !needsOnly ? (data?.built ?? []).filter(b => matches(b.name)).map(built => ({ key: `built:${built.id}`, built })) : []),
    ...(!['all', 'systems'].includes(filter) || needsOnly ? [] : (data?.systems ?? []).filter(s => matches(s.name)).map(system => ({ key: `system:${system.id}`, system }))),
    ...(!['all', 'services'].includes(filter) ? [] : (data?.apps ?? []).filter(app => !needsOnly || needsYou(app)).filter(app => matches(app.name)).map(service => ({ key: `service:${service.id}`, service }))),
  ];
  // The app that just connected: the one remembered before sign-in, else the newest.
  const connectedApp = arrival?.result !== 'connected' ? undefined : data?.apps.find(app => app.server === arrival.app)
    ?? (data?.apps ?? []).filter(app => app.connected).sort((a, b) => toMs(b.connected!.at) - toMs(a.connected!.at))[0];
  const preferred = connectedApp ? `service:${connectedApp.id}` : attention ? cards.find(card => card.service && needsYou(card.service))?.key : undefined;
  const current = cards.find(card => card.key === (selected ?? preferred)) ?? cards[0];
  const total = (data?.apps.length ?? 0) + (data?.systems.length ?? 0) + (data?.built?.length ?? 0);
  const changeRules = () => {
    if (connectedApp) setSelected(`service:${connectedApp.id}`);
    setArrival(undefined);
    // After the dialog hands focus back, move it to the rules that can now change.
    window.setTimeout(() => rulesHeading.current?.focus(), 0);
  };
  return <main className="apps-page" aria-label="Apps">
    <div className="apps-main">
      <header className="apps-header"><h1>Apps</h1>
        <div className="apps-tools">
          <label className="apps-search"><span className="visually-hidden">Search apps</span>
            <input type="search" placeholder="Search apps" value={query} onChange={event => setQuery(event.target.value)} /></label>
          {!!data?.requests_waiting && <Button onClick={() => setDeciding(true)}>{data.requests_waiting} request{data.requests_waiting === 1 ? '' : 's'}</Button>}
          {data && <Button onClick={() => setBuilding(true)}>Build an app</Button>}
          {data && <Button primary onClick={() => setAdding(true)}>{data.can_manage ? <><PlusIcon />Add app</> : 'Request an app'}</Button>}
        </div></header>
      <div className="apps-filters" role="group" aria-label="Show">
        {(['all', 'built', 'systems', 'services'] as Filter[]).map(option => <button key={option} type="button" aria-pressed={filter === option} onClick={() => setFilter(option)}>
          {{ all: 'All', built: 'Built here', systems: 'Your systems', services: 'Services' }[option]}</button>)}
        {needs > 0 && <button type="button" className="apps-needs" aria-pressed={needsOnly} onClick={() => setNeedsOnly(on => !on)}><span aria-hidden="true" />{needs} need{needs === 1 ? 's' : ''} you</button>}
      </div>
      {arrival?.result === 'failed' && <div className="apps-notice" role="alert"><span>The sign-in didn’t finish, so nothing was connected.</span>
        {data?.can_manage && <Button onClick={() => { setArrival(undefined); setAdding(true); }}>Try again</Button>}</div>}
      <p className="ui-status apps-status" role="status">{notice}</p>
      {resource.error && <><p role="alert">{resource.error}</p><Button onClick={refresh}>Retry</Button></>}
      {!data && !resource.error && <p role="status">Loading apps…</p>}
      {data && !total && <p className="ui-empty">{data.can_manage ? "No apps yet. Add your company's own systems, or a service such as Gmail so agents can use it under your rules." : 'No apps yet. Request the apps your team uses.'}</p>}
      {data && total > 0 && !cards.length && <p className="ui-empty">No apps match.</p>}
      <ul className="apps-grid">{cards.map(card => <li key={card.key}>
        <AppCard card={card} selected={current?.key === card.key} select={() => setSelected(card.key)} /></li>)}</ul>
    </div>
    {current?.built && <BuiltPanel app={current.built} changed={refresh} />}
    {current?.system && <SystemPanel app={current.system} canManage={Boolean(data?.can_manage)} changed={refresh} edit={() => setEditing(current.system)} />}
    {current?.service && <ServicePanel key={current.service.id} app={current.service} canManage={Boolean(data?.can_manage)} control={control.data} controlError={control.error}
      retryControl={() => void control.refresh().catch(() => {})} changed={changed} rulesHeading={rulesHeading}
      disconnected={message => { setNotice(message); setSelected(undefined); refresh(); }} />}
    {adding && data && <CatalogDialog canManage={data.can_manage} close={() => setAdding(false)} connect={app => { setAdding(false); setConnecting(app); }}
      open={connection => { setFilter('all'); setNeedsOnly(false); setSelected(`service:${connection}`); }}
      others={data.can_manage ? { addSystem: () => setEditing('new'), build: () => setBuilding(true) } : { suggest: () => setSuggesting(true), build: () => setBuilding(true) }} />}
    {connecting && <ConnectDialog app={connecting} close={() => setConnecting(undefined)}
      connected={() => { setArrival({ result: 'connected', app: connecting.id }); setConnecting(undefined); refresh(); }} />}
    {arrival?.result === 'connected' && data && <ConnectedDialog app={connectedApp} close={() => setArrival(undefined)} changeRules={changeRules}
      chooseAgents={() => { setArrival(undefined); navigate(workspaceUrl('/?view=agents')); }} />}
    {editing && <SystemForm app={editing === 'new' ? undefined : editing} close={() => setEditing(undefined)} saved={refresh} />}
    {building && <BuiltForm close={() => setBuilding(false)} saved={id => { setFilter('built'); setSelected(`built:${id}`); refresh(); }} />}
    {suggesting && <RequestDialog close={() => setSuggesting(false)} sent={refresh} />}
    {deciding && <RequestsDrawer close={() => setDeciding(false)} changed={refresh} />}
  </main>;
}
