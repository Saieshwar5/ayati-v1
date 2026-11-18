import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useCatalog } from './use-catalog';
import type { CatalogApp } from './apps-model';
import { AppLetter, CheckIcon, KindChips } from './app-marks';
import { RequestAppDialog } from './request-app-dialog';
import './catalog.css';

/** Add app, on an agent's Connections tab: Cuckoo's catalog of services,
 * searched and paged by the server. An app shows as connected when this agent
 * already has its own connection of it (schema 132). A member adds the apps an
 * admin allowed and asks for the others (schema 134). */
export function CatalogDialog({ agent, close, connect }: { agent: string; close: () => void; connect: (app: CatalogApp) => void }) {
  const [text, setText] = useState('');
  const [category, setCategory] = useState('');
  const catalog = useCatalog(text, category, agent);
  const [asking, ask] = useState<CatalogApp>();
  const total = catalog.categories?.reduce((sum, item) => sum + item.count, 0);
  return <Dialog title="Add app" close={close}>
    <div className="catalog">
      <nav className="catalog-nav" aria-label="Add app">
        <p className="catalog-heading">Services</p>
        <ul role="group" aria-label="Categories">
          <li><button type="button" aria-pressed={category === ''} onClick={() => setCategory('')}>All services<small>{total ?? ''}</small></button></li>
          {catalog.categories?.map(item => <li key={item.id}><button type="button" aria-pressed={category === item.id} onClick={() => setCategory(item.id)}>
            {item.name}<small>{item.count}</small></button></li>)}
        </ul>
      </nav>
      <div className="catalog-main">
        <div className="catalog-search">
          <label><span className="visually-hidden">Search services</span>
            <input type="search" placeholder={total ? `Search ${total} services` : 'Search services'} value={text} onChange={event => setText(event.target.value)} /></label>
          <span className="app-line">Apps you can add are listed first.</span>
        </div>
        {catalog.error && <p role="alert" className="catalog-error">{catalog.error} <Button onClick={catalog.reload}>Retry</Button></p>}
        {catalog.loading && !catalog.apps.length && !catalog.error && <p role="status">Loading apps…</p>}
        {!catalog.loading && !catalog.error && !catalog.apps.length && <p className="ui-empty">{catalog.query ? `No services match “${catalog.query}”.` : 'No services in this category yet.'}</p>}
        <ul className="catalog-tiles" aria-label="Services" aria-busy={catalog.loading}>{catalog.apps.map(app => <li key={app.id} className="catalog-tile">
          <div className="catalog-tile-top"><AppLetter id={app.id} name={app.name} size="small" />
            <span><strong>{app.name}</strong><small>{app.description}</small></span></div>
          <KindChips kinds={app.kinds} />
          <div className="catalog-tile-foot">
            {app.state === 'connected' ? <span className="catalog-state ok"><CheckIcon />Connected</span>
              : app.state === 'available' ? <Button aria-label={`Connect ${app.name}`} onClick={() => connect(app)}>Connect</Button>
              : app.state === 'ask' ? <Button aria-label={`Ask an admin for ${app.name}`} onClick={() => ask(app)}>Ask admin</Button>
              : app.state === 'requested' ? <span className="catalog-state">Asked · waiting for an admin</span>
              : <span className="catalog-state">Not available here</span>}
            <span>{app.actions} action{app.actions === 1 ? '' : 's'}</span>
          </div>
          {app.state === 'blocked' && <p className="catalog-tile-note">{app.note ? `An admin switched it off: ${app.note}` : 'An admin switched it off for this workspace.'}</p>}
          {app.state === 'ask' && <p className="catalog-tile-note">Needs an admin’s yes first.</p>}
        </li>)}</ul>
        {catalog.next && <Button className="catalog-more" disabled={catalog.paging.busy} onClick={catalog.more}>{catalog.paging.busy ? 'Loading…' : 'Load more'}</Button>}
        {catalog.paging.error && <p role="alert">{catalog.paging.error}</p>}
        <p className="catalog-help"><InfoMark /><span>Can’t find an app? Cuckoo adds apps to this list over time. For your company’s own system, add an MCP server instead.</span></p>
      </div>
    </div>
    {asking && <RequestAppDialog app={asking} agent={agent} close={() => ask(undefined)} sent={() => { ask(undefined); catalog.reload(); }} />}
  </Dialog>;
}

const mark = (path: string) => <svg className="mark-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={path} /></svg>;
const InfoMark = () => mark('M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12ZM8 7.5V11M8 5h.01');
