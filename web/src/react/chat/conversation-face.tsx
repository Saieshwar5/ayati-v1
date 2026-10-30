import { AgentFace, AiTag, PersonFace } from '../faces';
import { directPartner, type Conversation, type Directory, type Participant } from './model';

/** A channel shows # (or a lock when private); a direct message shows the other side. */
export function ConversationFace({ chat, directory }: { chat: Conversation; directory: Directory }) {
  const partner = directPartner(chat, directory);
  if (partner) return <ParticipantFace kind={partner.kind} who={partner.who} />;
  return <span className="chat-avatar channel" aria-hidden="true">{chat.visibility === 'public' ? '#' : <LockIcon />}</span>;
}

export function ParticipantFace({ kind, who }: { kind: 'user' | 'agent'; who: Participant }) {
  return kind === 'agent' ? <AgentFace agent={who} /> : <PersonFace person={who} />;
}

/** The name with an AI tag for agents, so people and agents read the same way. */
export function ParticipantName({ kind, name }: { kind: 'user' | 'agent'; name: string }) {
  return <span className="chat-participant-name"><span className="chat-name-text">{name}</span>{kind === 'agent' && <AiTag />}</span>;
}

function LockIcon() {
  return <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
    <rect x="5" y="11" width="14" height="9" rx="2" /><path d="M8 11V8a4 4 0 0 1 8 0v3" />
  </svg>;
}
