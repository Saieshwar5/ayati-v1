import { useState, type ReactNode } from 'react';
import { Button, Dialog } from '../ui';
import { RequestDialog } from './company-apps';
import { useCatalog } from './use-catalog';
import type { CatalogApp } from './apps-model';
import { AppLetter, CheckIcon, KindChips } from './app-marks';
import './catalog.css';

type OtherWays = { addSystem?: () => void; build?: () => void; suggest?: () => void };

/** Add app (designs/connector-screens 2): every kind of connection in one dialog.
 * Services come from Cuckoo's catalog, searched and paged by the server; below them
 * MCP servers (next), the company's own apps and built apps. Members browse the same
 * catalog and request an app instead of connecting it. */
export function CatalogDialog({ canManage, close, connect, open, others = {} }: {
  canManage: boolean; close: () => void; connect: (app: CatalogApp) => void; open?: (connection: string) => void; others?: OtherWays;
}) {
  const [text, setText] = useState('');
  const [category, setCategory] = useState('');
  const catalog = useCatalog(text, category);
  const [requesting, setRequesting] = useState<CatalogApp>();
  const total = catalog.categories?.reduce((sum, item) => sum + item.count, 0);
  const leave = (next?: () => void) => next && (() => { close(); next(); });
  const ways: [string, (() => void) | undefined, ReactNode][] = [
    ["Your company's app", leave(others.addSystem), <LinkMark />], ['Suggest another app', leave(others.suggest), <LinkMark />], ['Build an app', leave(others.build), <BuildMark />]];
  return <Dialog title={canManage ? 'Add app' : 'Request an app'} close={close}>
    <div className="catalog">
      <nav className="catalog-nav" aria-label="Add app">
        <p className="catalog-heading">Services</p>
        <ul role="group" aria-label="Categories">
          <li><button type="button" aria-pressed={category === ''} onClick={() => setCategory('')}>All services<small>{total ?? ''}</small></button></li>
          {catalog.categories?.map(item => <li key={item.id}><button type="button" aria-pressed={category === item.id} onClick={() => setCategory(item.id)}>
            {item.name}<small>{item.count}</small></button></li>)}
        </ul>
        <p className="catalog-heading">Other ways to add</p>
        <ul>
          {canManage && <li><span className="catalog-other" aria-disabled="true"><ServerMark />MCP server<span className="catalog-next">Next</span></span></li>}
          {ways.filter(([, act]) => act).map(([label, act, icon]) => <li key={label}><button type="button" onClick={act}>{icon}{label}</button></li>)}
        </ul>
      </nav>
      <div className="catalog-main">
        <div className="catalog-search">
          <label><span className="visually-hidden">Search services</span>
            <input type="search" placeholder={total ? `Search ${total} services` : 'Search services'} value={text} onChange={event => setText(event.target.value)} /></label>
          <span className="app-line">Connected apps are listed first.</span>
        </div>
        {catalog.error && <p role="alert" className="catalog-error">{catalog.error} <Button onClick={catalog.reload}>Retry</Button></p>}
        {catalog.loading && !catalog.apps.length && !catalog.error && <p role="status">Loading apps…</p>}
        {!catalog.loading && !catalog.error && !catalog.apps.length && <p className="ui-empty">{catalog.query ? `No services match “${catalog.query}”.` : 'No services in this category yet.'}</p>}
        <ul className="catalog-tiles" aria-label="Services" aria-busy={catalog.loading}>{catalog.apps.map(app => <li key={app.id} className="catalog-tile">
          <div className="catalog-tile-top"><AppLetter id={app.id} name={app.name} size="small" />
            <span><strong>{app.name}</strong><small>{app.description}</small></span></div>
          <KindChips kinds={app.kinds} />
          <div className="catalog-tile-foot">
            <TileAction app={app} canManage={canManage} connect={connect} open={open && (connection => { close(); open(connection); })} request={setRequesting} />
            <span>{!canManage && app.state === 'available' ? 'Admins decide' : !canManage && app.state === 'requested' ? 'You asked' : `${app.actions} action${app.actions === 1 ? '' : 's'}`}</span>
          </div>
        </li>)}</ul>
        {catalog.next && <Button className="catalog-more" disabled={catalog.paging.busy} onClick={catalog.more}>{catalog.paging.busy ? 'Loading…' : 'Load more'}</Button>}
        {catalog.paging.error && <p role="alert">{catalog.paging.error}</p>}
        <p className="catalog-help"><InfoMark /><span>{canManage
          ? 'Can’t find an app? Cuckoo adds apps to this list over time, and adding an MCP server comes next. Until then, add it as your company’s app.'
          : 'Can’t find an app? Suggest it, and your admins decide.'}</span></p>
      </div>
    </div>
    {requesting && <RequestDialog suggest={requesting.name} close={() => setRequesting(undefined)} sent={catalog.reload} />}
  </Dialog>;
}

function TileAction({ app, canManage, connect, open, request }: {
  app: CatalogApp; canManage: boolean; connect: (app: CatalogApp) => void; open?: (connection: string) => void; request: (app: CatalogApp) => void;
}) {
  if (app.state === 'connected') return open && app.connection
    ? <button type="button" className="catalog-state ok" aria-label={`${app.name} is connected. Open its panel`} onClick={() => open(app.connection!)}><CheckIcon />Connected</button>
    : <span className="catalog-state ok"><CheckIcon />Connected</span>;
  if (canManage) return <span className="catalog-actions"><Button aria-label={`Connect ${app.name}`} onClick={() => connect(app)}>Connect</Button>
    {app.state === 'requested' && <span className="catalog-state">Requested</span>}</span>;
  return app.state === 'requested' ? <span className="catalog-state">Requested</span>
    : <Button aria-label={`Request ${app.name}`} onClick={() => request(app)}>Request</Button>;
}

const mark = (path: string) => <svg className="mark-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={path} /></svg>;
const ServerMark = () => mark('M3 3.5h10v4H3zM3 8.5h10v4H3zM5.5 5.5h.01M5.5 10.5h.01');
const LinkMark = () => mark('M7 9a2.5 2.5 0 0 0 3.5 0l2-2A2.5 2.5 0 0 0 9 3.5l-.7.7M9 7a2.5 2.5 0 0 0-3.5 0l-2 2A2.5 2.5 0 0 0 7 12.5l.7-.7');
const BuildMark = () => mark('M5.5 5 2.5 8l3 3M10.5 5l3 3-3 3M9 3.5l-2 9');
const InfoMark = () => mark('M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12ZM8 7.5V11M8 5h.01');
