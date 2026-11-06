import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useResource } from '../requests';
import { AgentFace } from '../faces';
import { modes } from '../agents/control-model';
import { meanings } from '../settings/workspace-connections-model';
import { WorkspaceConnections } from '../settings/workspace-connections';
import { workspaceUrl } from '../../workspace-context';
import { appColour, loadApps, type CompanySystem, type WorkspaceApp } from './apps-model';
import { LinkIcon, RequestDialog, RequestsDrawer, SystemForm, SystemPanel } from './company-apps';
import './apps.css';

function Plug() {
  return <svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0ZM12 17v4" /></svg>;
}

type Filter = 'all' | 'systems' | 'services';
type Card = { key: string; system?: CompanySystem; service?: WorkspaceApp };

/** Apps (apps-page plan, AP1–AP3): the company's own systems, which open through
 * Cuckoo for the people allowed, and every connected service with what agents may do.
 * Everyone reads it and opens or requests apps; admins add and manage them. */
export function AppsPage({ href }: { href: string }) {
  const url = new URL(href);
  const resource = useResource(loadApps);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const [needsOnly, setNeedsOnly] = useState(false);
  const [selected, setSelected] = useState<string>();
  const [managing, setManaging] = useState(url.searchParams.get('manage') === '1' || url.searchParams.get('section') === 'connections');
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<CompanySystem | 'new'>();
  const [suggesting, setSuggesting] = useState(false);
  const [deciding, setDeciding] = useState(false);
  const data = resource.data;
  const refresh = () => void resource.refresh().catch(() => {});
  const needs = data?.apps.filter(app => app.status === 'reconnect').length ?? 0;
  const matches = (name: string) => !query.trim() || name.toLowerCase().includes(query.trim().toLowerCase());
  const cards: Card[] = [
    ...(filter === 'services' || needsOnly ? [] : (data?.systems ?? []).filter(s => matches(s.name)).map(system => ({ key: `system:${system.id}`, system }))),
    ...(filter === 'systems' ? [] : (data?.apps ?? []).filter(app => !needsOnly || app.status === 'reconnect').filter(app => matches(app.name)).map(service => ({ key: `service:${service.id}`, service }))),
  ];
  const current = cards.find(card => card.key === selected) ?? cards[0];
  const close = () => { setManaging(false); refresh(); };
  const total = (data?.apps.length ?? 0) + (data?.systems.length ?? 0);
  return <main className="apps-page" aria-label="Apps">
    <div className="apps-main">
      <header className="apps-header"><h1>Apps</h1>
        <div className="apps-tools">
          <label className="apps-search"><span className="visually-hidden">Search apps</span>
            <input type="search" placeholder="Search apps" value={query} onChange={event => setQuery(event.target.value)} /></label>
          {!!data?.requests_waiting && <Button onClick={() => setDeciding(true)}>{data.requests_waiting} request{data.requests_waiting === 1 ? '' : 's'}</Button>}
          {data && !data.can_manage && <Button onClick={() => setSuggesting(true)}>Suggest an app</Button>}
          {data?.can_manage && <Button primary onClick={() => setAdding(true)}>Add app</Button>}
        </div></header>
      <div className="apps-filters" role="group" aria-label="Show">
        {(['all', 'systems', 'services'] as Filter[]).map(option => <button key={option} type="button" aria-pressed={filter === option} onClick={() => setFilter(option)}>
          {{ all: 'All', systems: 'Your systems', services: 'Services' }[option]}</button>)}
        {needs > 0 && <button type="button" className="apps-needs" aria-pressed={needsOnly} onClick={() => setNeedsOnly(on => !on)}><span aria-hidden="true" />{needs} need{needs === 1 ? 's' : ''} you</button>}
      </div>
      {resource.error && <><p role="alert">{resource.error}</p><Button onClick={refresh}>Retry</Button></>}
      {!data && !resource.error && <p role="status">Loading apps…</p>}
      {data && !total && <p className="ui-empty">{data.can_manage ? "No apps yet. Add your company's own systems, or a service such as Gmail so agents can use it under your rules." : 'No apps yet. Suggest the apps your team uses.'}</p>}
      {data && total > 0 && !cards.length && <p className="ui-empty">No apps match.</p>}
      <ul className="apps-grid">{cards.map(card => <li key={card.key}>
        <button type="button" className="app-card" aria-pressed={current?.key === card.key} onClick={() => setSelected(card.key)}>
          {card.system ? <>
            <span className="app-card-top"><span className="app-icon" style={{ background: appColour(card.system.id) }}><LinkIcon /></span>
              {!card.system.can_open && <span className="app-status quiet">{card.system.requested ? 'Requested' : 'No access'}</span>}</span>
            <span className="app-name">{card.system.name}</span>
            <span className="app-line">{card.system.description || 'Your system'}</span>
            <span className="app-foot"><span>{card.system.audience === 'everyone' ? 'Everyone' : 'Chosen people'}</span><span>Link</span></span>
          </> : card.service && <>
            <span className="app-card-top"><span className="app-icon" style={{ background: appColour(card.service.server) }}><Plug /></span>
              {card.service.status === 'reconnect' && <span className="app-status">Reconnect</span>}</span>
            <span className="app-name">{card.service.name}</span>
            <span className="app-line">{card.service.account ?? 'Connected service'}</span>
            <span className="app-foot"><span className="app-faces">{card.service.agents.slice(0, 4).map(agent => <AgentFace key={agent.id} agent={agent} className="app-face" />)}
              {!card.service.agents.length && 'No agents yet'}</span><span>Service</span></span>
          </>}
        </button></li>)}</ul>
    </div>
    {current?.system && <SystemPanel app={current.system} canManage={Boolean(data?.can_manage)} changed={refresh} edit={() => setEditing(current.system)} />}
    {current?.service && <AppPanel app={current.service} canManage={Boolean(data?.can_manage)} manage={() => setManaging(true)} />}
    {adding && <Dialog title="Add app" close={() => setAdding(false)}>
      <div className="apps-choices">
        <button type="button" onClick={() => { setAdding(false); setEditing('new'); }}><strong>Add your company's app</strong><span>A link to a system you already run, opened through Cuckoo for the people you choose.</span></button>
        <button type="button" onClick={() => { setAdding(false); setManaging(true); }}><strong>Connect a service</strong><span>Gmail, Zoho Books and others, for agents to use under your rules.</span></button>
      </div></Dialog>}
    {editing && <SystemForm app={editing === 'new' ? undefined : editing} close={() => setEditing(undefined)} saved={refresh} />}
    {suggesting && <RequestDialog close={() => setSuggesting(false)} sent={refresh} />}
    {deciding && <RequestsDrawer close={() => setDeciding(false)} changed={refresh} />}
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
