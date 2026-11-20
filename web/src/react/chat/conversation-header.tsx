import type { ReactNode } from 'react';
import { Button } from '../ui';
import { ConversationFace, ParticipantFace, ParticipantName } from './conversation-face';
import { directPartner, type Conversation, type Directory } from './model';
import { NotifyMenu } from './notify-controls';
import type { Level } from './notifications';
import { PresenceLine } from './presence-view';
import { agentStateText, takesMessages } from './agent-state';
import { AgentSettingsLink } from './assistant-settings';

/** How the conversation reaches the side panel: open state for the toggle, show, and the archive review. */
export type DetailsControl = { open: boolean; toggle: () => void; show: () => void; review: () => void };

/** The connection is no longer shown here; it appears as a banner only while down (D245).
 * Since 2026-11-20 the face and name are the way to the profile or details (a ▾
 * says so), as on phones; there is no separate Profile or Details button. */
export function ConversationHeader({ chat, directory, title, back, returnTo, search, level, levelChanged, details, extra, working }: {
  chat: Conversation; directory: Directory; title: string; back: () => void; search: () => void;
  /** Opened from Chat → For you: a link back to it, in place of the phone's ‹ Chats. */
  returnTo?: string;
  /** Shown before the search button: what your assistant is doing in this chat. */
  extra?: ReactNode;
  /** In a direct message with an agent: what it is doing for you here (agent-state.tsx). */
  working?: string;
  level?: Level; levelChanged: (level: Level) => void; details: DetailsControl;
}) {
  const partner = directPartner(chat, directory);
  const person = partner?.kind === 'user' ? partner.who.id : undefined;
  const agent = partner?.kind === 'agent' ? partner.who : undefined;
  const summary = partner ? (partner.kind === 'agent' ? partner.who.role : partner.who.title || 'Direct message')
    : [chat.visibility === 'public' ? 'Public channel' : 'Private channel', `${chat.users.length} ${chat.users.length === 1 ? 'person' : 'people'}`,
      chat.agents.length ? `${chat.agents.length} ${chat.agents.length === 1 ? 'agent' : 'agents'}` : ''].filter(Boolean).join(' · ');
  const panel = chat.kind === 'direct' ? 'profile' : 'details';
  // Its owners and admins reach a team agent's page from here; your assistant has its own link.
  const settings = agent && !agent.assistant && (agent.yours || directory.role === 'Admin');
  return <header className="chat-pane-header">
    {returnTo ? <a className="chat-return" href={returnTo}>‹ For you</a> : <Button className="chat-back" onClick={back}>‹ Chats</Button>}
    {/* The whole block opens the panel; the two buttons inside carry it for keyboards and screen readers. */}
    <div className="chat-header-who" onClick={event => { if (!(event.target as Element).closest('button, a')) details.toggle(); }}>
      {partner ? <button type="button" className="chat-header-identity" aria-label={`View profile of ${title}`} aria-expanded={details.open} onClick={details.toggle}>
        <ConversationFace chat={chat} directory={directory} working={Boolean(working)} />
      </button> : <ConversationFace chat={chat} directory={directory} />}
      <div className="chat-header-text">
        <h2 title={title}><button type="button" className="chat-header-name" aria-label={`${title}, ${panel}`} aria-expanded={details.open} onClick={details.toggle}>
          {partner ? <ParticipantName kind={partner.kind} name={title} /> : title}
          <svg className="chat-header-chevron" viewBox="0 0 12 12" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><path d="m3 4.5 3 3 3-3" /></svg>
        </button></h2>
        <div className="chat-header-meta">
          {person ? <PresenceLine id={person} fallback={summary ?? ''} />
            : agent ? <AgentLine agent={agent} working={working} />
            : <small className="chat-kind">{summary}</small>}
        </div>
      </div>
    </div>
    {extra}
    {settings && <AgentSettingsLink agent={agent.id} name={agent.name} />}
    <Button className="ui-icon chat-search-button" aria-label={`Search in ${title}`} title="Search this conversation" onClick={search}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
    </Button>
    {chat.kind !== 'direct' && <HeaderFaces chat={chat} directory={directory} details={details} />}
    <NotifyMenu chat={chat} level={level} changed={levelChanged} />
  </header>;
}

/** "● Ready · Answers customer questions", "● Working on your request…" or "○ Paused · Ananya Rao can resume it". */
function AgentLine({ agent, working }: { agent: NonNullable<ReturnType<typeof directPartner>>['who']; working?: string }) {
  const ready = takesMessages(agent);
  const { label, detail } = agentStateText(agent);
  const tone = ready ? (working ? 'working' : 'ready') : 'paused';
  return <small className="chat-kind agent-line">
    <span className={`agent-line-dot agent-dot-${tone}`} aria-hidden="true" />
    <span className="agent-line-label">{ready && working ? working : label}</span>
    {!(ready && working) && detail && <span className="agent-line-detail"> · {detail}</span>}
  </small>;
}

/** Who is in the channel, before the bell (D245 screen 5, note 6); agents show their state dot. */
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
    {shown.map(({ kind, who }) => <ParticipantFace key={`${kind}:${who.id}`} kind={kind} who={who} presence={kind === 'agent'} />)}
  </button>;
}
