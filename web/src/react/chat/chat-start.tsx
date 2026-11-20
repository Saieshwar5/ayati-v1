import { workspaceUrl } from '../../workspace-context';
import { AgentFace, AiTag, initials, PersonFace } from '../faces';
import { PageIcon } from '../workspace-shell';
import type { DirectTarget } from './direct-conversation';
import type { PendingInvite, StartFacts } from './start-model';
import './chat-start.css';

export const inviteHref = () => workspaceUrl('/?view=settings&section=people');
export const createAgentHref = () => workspaceUrl('/?view=agents&new=1');
const FACES = 6;

/** Invite people (admins only): your invitations still waiting show as faded faces. */
export function InviteTile({ invites, primary = false }: { invites: PendingInvite[]; primary?: boolean }) {
  const waiting = invites.slice(0, 3);
  const label = invites.length ? `Invite people, ${invites.length} ${invites.length === 1 ? 'invitation' : 'invitations'} waiting` : 'Invite people';
  return <a className={`chat-start-tile${primary ? ' chat-start-tile-primary' : ''}`} href={inviteHref()} aria-label={label}>
    <span className="chat-start-icons" aria-hidden="true">
      {waiting.map(invite => <span key={invite.id} className="chat-start-face chat-start-waiting" title={invite.email}>{initials(invite.email.split('@')[0])}</span>)}
      <span className="chat-start-face chat-start-add">+</span>
    </span>
    <span>Invite people</span>
  </a>;
}

export function CreateAgentTile({ row = false }: { row?: boolean }) {
  return <a className={`chat-start-tile${row ? ' chat-start-tile-row' : ''}`} href={createAgentHref()}>
    <span className="chat-start-icons" aria-hidden="true"><span className="chat-start-agent"><PageIcon name="agents" /><i>+</i></span></span>
    <span>Create agent</span>
  </a>;
}

/** The people already here: one click opens a direct message; All opens People & agents. */
function PeopleTile({ people, message, showAll }: { people: StartFacts['people']; message: (who: DirectTarget) => void; showAll: () => void }) {
  const shown = people.slice(0, FACES);
  return <div className="chat-start-tile chat-start-tile-wide" role="group" aria-label="People">
    <ul className="chat-start-people">
      {shown.map(person => <li key={person.id}><button type="button" onClick={() => message({ ...person, kind: 'user' })} aria-label={`Message ${person.name}`}>
        <PersonFace person={person} /><span>{person.name.split(' ')[0]}</span>
      </button></li>)}
      <li><button type="button" onClick={showAll} aria-label="All people and agents">
        <span className="chat-avatar chat-start-add" aria-hidden="true">{people.length > shown.length ? `+${people.length - shown.length}` : '…'}</span><span>All</span>
      </button></li>
    </ul>
  </div>;
}

/** The tiles for what you can do next, by what you may do: Invite is for admins. */
export function StartTiles({ facts, invites, message, showAll, compact = false }: {
  facts: StartFacts; invites: PendingInvite[]; message: (who: DirectTarget) => void; showAll: () => void; compact?: boolean;
}) {
  const people = facts.people.length > 0 && !compact;
  return <div className={`chat-start-tiles${compact ? ' chat-start-compact' : ''}`}>
    {people && <PeopleTile people={facts.people} message={message} showAll={showAll} />}
    {facts.admin && <InviteTile invites={invites} />}
    <CreateAgentTile row={!facts.admin} />
  </div>;
}

/** Chat with nothing open: your assistant first, then the people and agents to work with. */
export function ChatStart({ facts, invites, openAssistant, message, showAll }: {
  facts: StartFacts; invites: PendingInvite[]; openAssistant?: () => void; message: (who: DirectTarget) => void; showAll: () => void;
}) {
  const assistant = facts.assistant;
  return <section className="chat-start" aria-label="Start">
    {assistant && openAssistant && <>
      <div className="chat-start-hero"><AgentFace agent={assistant} className="chat-avatar chat-start-hero-face" /><h2>{assistant.name} <AiTag /></h2></div>
      <button type="button" className="chat-start-ask" onClick={openAssistant} aria-label={`Ask ${assistant.name}`}>
        <span>Ask {assistant.name} anything…</span>
        <span className="chat-start-send" aria-hidden="true"><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 19V5M6 11l6-6 6 6" /></svg></span>
      </button>
    </>}
    <StartTiles facts={facts} invites={invites} message={message} showAll={showAll} />
  </section>;
}
