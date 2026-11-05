import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useResource } from '../requests';
import { AgentFace } from '../faces';
import { modes } from '../agents/control-model';
import { meanings } from '../settings/workspace-connections-model';
import { WorkspaceConnections } from '../settings/workspace-connections';
import { workspaceUrl } from '../../workspace-context';
import { appColour, loadApps, type WorkspaceApp } from './apps-model';
import './apps.css';

function Plug() {
  return <svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0ZM12 17v4" /></svg>;
}

/** Apps (apps-page plan, AP1): every service the company connected, what agents may do
 * with each action and who uses it. Everyone reads it; admins add and manage apps. */
export function AppsPage({ href }: { href: string }) {
  const url = new URL(href);
  const resource = useResource(loadApps);
  const [query, setQuery] = useState('');
  const [needsOnly, setNeedsOnly] = useState(false);
  const [selected, setSelected] = useState<string>();
  const [managing, setManaging] = useState(url.searchParams.get('manage') === '1' || url.searchParams.get('section') === 'connections');
  const data = resource.data;
  const needs = data?.apps.filter(app => app.status === 'reconnect').length ?? 0;
  const apps = (data?.apps ?? []).filter(app => !needsOnly || app.status === 'reconnect')
    .filter(app => !query.trim() || app.name.toLowerCase().includes(query.trim().toLowerCase()));
  const current = apps.find(app => app.id === selected) ?? apps[0];
  const close = () => { setManaging(false); void resource.refresh().catch(() => {}); };
  return <main className="apps-page" aria-label="Apps">
    <div className="apps-main">
      <header className="apps-header"><h1>Apps</h1>
        <div className="apps-tools">
          <label className="apps-search"><span className="visually-hidden">Search apps</span>
            <input type="search" placeholder="Search apps" value={query} onChange={event => setQuery(event.target.value)} /></label>
          {data?.can_manage && <Button primary onClick={() => setManaging(true)}>Add app</Button>}
        </div></header>
      {needs > 0 && <button type="button" className="apps-needs" aria-pressed={needsOnly} onClick={() => setNeedsOnly(on => !on)}><span aria-hidden="true" />{needs} need{needs === 1 ? 's' : ''} you</button>}
      {resource.error && <><p role="alert">{resource.error}</p><Button onClick={() => void resource.refresh().catch(() => {})}>Retry</Button></>}
      {!data && !resource.error && <p role="status">Loading apps…</p>}
      {data && !data.apps.length && <p className="ui-empty">{data.can_manage ? 'No apps yet. Add a service such as Gmail so agents can use it under your rules.' : 'No apps yet. Ask an admin to add the services your company uses.'}</p>}
      {data && data.apps.length > 0 && !apps.length && <p className="ui-empty">No apps match.</p>}
      <ul className="apps-grid">{apps.map(app => <li key={app.id}>
        <button type="button" className="app-card" aria-pressed={current?.id === app.id} onClick={() => setSelected(app.id)}>
          <span className="app-card-top"><span className="app-icon" style={{ background: appColour(app.server) }}><Plug /></span>
            {app.status === 'reconnect' && <span className="app-status">Reconnect</span>}</span>
          <span className="app-name">{app.name}</span>
          <span className="app-line">{app.account ?? 'Connected service'}</span>
          <span className="app-foot"><span className="app-faces">{app.agents.slice(0, 4).map(agent => <AgentFace key={agent.id} agent={agent} className="app-face" />)}
            {!app.agents.length && 'No agents yet'}</span><span>Service</span></span>
        </button></li>)}</ul>
    </div>
    {current && <AppPanel app={current} canManage={Boolean(data?.can_manage)} manage={() => setManaging(true)} />}
    {managing && data?.can_manage && <Dialog drawer title="Add and manage apps" close={close}><WorkspaceConnections /></Dialog>}
  </main>;
}

function AppPanel({ app, canManage, manage }: { app: WorkspaceApp; canManage: boolean; manage: () => void }) {
  return <aside className="app-panel" aria-label={app.name}>
    <span className="app-icon big" style={{ background: appColour(app.server) }}><Plug /></span>
    <div><h2>{app.name}</h2><p className="app-line">Service · {app.status === 'reconnect' ? 'Needs reconnecting' : 'Connected'}</p></div>
    {app.status === 'reconnect' && <p className="app-callout">The sign-in expired. Agents can't use {app.name} until {canManage ? 'you reconnect it' : 'an admin reconnects it'}.</p>}
    {canManage && <Button onClick={manage}>{app.status === 'reconnect' ? 'Reconnect' : 'Manage'}</Button>}
    <section aria-label="Agents can"><h3>Agents can</h3>
      <ul className="app-actions">{app.actions.map(action => <li key={action.id}>
        <span>{action.title}<small>{meanings[action.meaning]}</small></span><span className={`app-mode ${action.mode}`}>{modes[action.mode]}</span></li>)}</ul></section>
    <section aria-label="Used by"><h3>Used by</h3>
      <p>{app.agents.length ? app.agents.map(agent => agent.name).join(', ') : 'No agent uses it yet. An agent\'s owner switches it on from the agent\'s Tools tab.'}</p></section>
    <section aria-label="Company rules"><h3>Company rules</h3>
      <p>{app.rules ? <a href={workspaceUrl('/?view=settings&section=company-rules')}>{app.rules} rule{app.rules === 1 ? '' : 's'} cover this app</a> : 'No company rules cover this app yet.'}</p></section>
  </aside>;
}
