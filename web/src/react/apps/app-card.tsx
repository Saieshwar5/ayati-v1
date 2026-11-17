import { AgentFace } from '../faces';
import { agoText, appColour, appState, dayName, type CompanySystem, type WorkspaceApp } from './apps-model';
import { AppLetter, PauseIcon, RefreshIcon } from './app-marks';
import { LinkIcon } from './company-apps';

export type Card = { key: string; system?: CompanySystem; service?: WorkspaceApp };

/** One tile on Apps. A service names the one thing that needs attention, and when
 * it was last used (designs/connector-screens 1 and 6). */
export function AppCard({ card, selected, select }: { card: Card; selected: boolean; select: () => void }) {
  return <button type="button" className="app-card" aria-pressed={selected} onClick={select}>
    {card.system ? <>
      <span className="app-card-top"><span className="app-icon" style={{ background: appColour(card.system.id) }}><LinkIcon /></span>
        {!card.system.can_open && <span className="app-status quiet">{card.system.requested ? 'Requested' : 'No access'}</span>}</span>
      <span className="app-name">{card.system.name}</span>
      <span className="app-line">{card.system.description || 'Your system'}</span>
      <span className="app-foot"><span>{card.system.audience === 'everyone' ? 'Everyone' : 'Chosen people'}</span><span>Link</span></span>
    </> : card.service && <ServiceCard app={card.service} />}
  </button>;
}

function ServiceCard({ app }: { app: WorkspaceApp }) {
  const state = appState(app);
  const unused = state === 'ok' && !app.agents.length && !app.last_used;
  return <>
    <span className="app-card-top"><AppLetter id={app.server} name={app.name} />
      {state === 'writes_paused' && <span className="app-status"><PauseIcon />Writes paused</span>}
      {state === 'reconnect' && <span className="app-status"><RefreshIcon />Reconnect</span>}
      {state === 'paused' && <span className="app-status quiet">Paused by Cuckoo</span>}
      {unused && <span className="app-status quiet">Not used yet</span>}</span>
    <span className="app-name">{app.name}</span>
    <span className="app-line">{app.account ?? 'Connected service'}</span>
    <span className="app-foot"><span className="app-faces">{app.agents.slice(0, 4).map(agent => <AgentFace key={agent.id} agent={agent} className="app-face" />)}
      {!app.agents.length && 'No agents yet'}</span>
      <span>{app.last_used ? `Used ${agoText(app.last_used.at)}` : app.connected ? `Connected ${dayName(app.connected.at)}` : 'Service'}</span></span>
  </>;
}
