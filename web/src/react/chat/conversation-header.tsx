import { Button } from '../ui';
import { ConversationFace, ParticipantFace, ParticipantName } from './conversation-face';
import { directPartner, type Conversation, type Directory } from './model';
import { NotifyMenu } from './notify-controls';
import type { Level } from './notifications';
import { PresenceLine } from './presence-view';

/** How the conversation reaches the side panel: open state for the toggle, show, and the archive review. */
export type DetailsControl = { open: boolean; toggle: () => void; show: () => void; review: () => void };

/** The connection is no longer shown here; it appears as a banner only while down (D245). */
export function ConversationHeader({ chat, directory, title, back, search, level, levelChanged, details }: {
  chat: Conversation; directory: Directory; title: string; back: () => void; search: () => void;
  level?: Level; levelChanged: (level: Level) => void; details: DetailsControl;
}) {
  const partner = directPartner(chat, directory);
  const person = partner?.kind === 'user' ? partner.who.id : undefined;
  const summary = partner ? (partner.kind === 'agent' ? partner.who.role : partner.who.title || 'Direct message')
    : [chat.visibility === 'public' ? 'Public channel' : 'Private channel', `${chat.users.length} ${chat.users.length === 1 ? 'person' : 'people'}`,
      chat.agents.length ? `${chat.agents.length} ${chat.agents.length === 1 ? 'agent' : 'agents'}` : ''].filter(Boolean).join(' · ');
  return <header className="chat-pane-header">
    <Button className="chat-back" onClick={back}>‹ Chats</Button>
    {partner ? <button type="button" className="chat-header-identity" aria-label={`View profile of ${title}`} onClick={details.show}>
      <ConversationFace chat={chat} directory={directory} />
    </button> : <ConversationFace chat={chat} directory={directory} />}
    <div>
      <h2 title={title}>{partner ? <ParticipantName kind={partner.kind} name={title} /> : title}</h2>
      <div className="chat-header-meta">
        {person ? <PresenceLine id={person} fallback={summary ?? ''} /> : <small className="chat-kind">{summary}</small>}
      </div>
    </div>
    <Button className="ui-icon chat-search-button" aria-label={`Search in ${title}`} title="Search this conversation" onClick={search}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
    </Button>
    {chat.kind !== 'direct' && <HeaderFaces chat={chat} directory={directory} details={details} />}
    <NotifyMenu chat={chat} level={level} changed={levelChanged} />
    <Button className="chat-details-toggle" aria-expanded={details.open} onClick={details.toggle}>{chat.kind === 'direct' ? 'Profile' : 'Details'}</Button>
  </header>;
}

/** Who is in the channel, before the bell and Details (D245 screen 5, note 6). */
function HeaderFaces({ chat, directory, details }: { chat: Conversation; directory: Directory; details: DetailsControl }) {
  const people = directory.users.filter(user => chat.users.includes(user.id));
  const agents = directory.agents.filter(agent => chat.agents.includes(agent.id));
  const shown = [
    ...people.slice(0, agents.length ? 3 : 4).map(who => ({ kind: 'user' as const, who })),
    ...agents.slice(0, 1).map(who => ({ kind: 'agent' as const, who })),
  ];
  if (!shown.length) return null;
  return <button type="button" className="chat-header-faces" aria-label={`People and agents in ${chat.name}`} aria-expanded={details.open}
    title="People and agents" onClick={details.toggle}>
    {shown.map(({ kind, who }) => <ParticipantFace key={`${kind}:${who.id}`} kind={kind} who={who} />)}
  </button>;
}
