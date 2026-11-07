import { AgentFace, AiTag, PersonFace } from '../faces';
import { directPartner, type Conversation, type Directory, type Participant } from './model';
import { PresenceDot } from './presence-view';

/** A channel shows # (or a lock when private); a direct message shows the other side, with their online dot. */
export function ConversationFace({ chat, directory }: { chat: Conversation; directory: Directory }) {
  const partner = directPartner(chat, directory);
  if (partner) return <ParticipantFace kind={partner.kind} who={partner.who} presence />;
  return <span className="chat-avatar channel" aria-hidden="true">{chat.visibility === 'public' ? '#' : <LockIcon />}</span>;
}

/** `presence` adds a person's green dot (D245); agents never have one. */
export function ParticipantFace({ kind, who, presence = false }: { kind: 'user' | 'agent'; who: Participant; presence?: boolean }) {
  if (kind === 'agent') return <AgentFace agent={who} />;
  // The dot subscribes on its own, so a change rerenders only the faces showing that person.
  return <PersonFace person={who} badge={presence ? <PresenceDot id={who.id} /> : undefined} />;
}

/** The name with an AI tag for agents, so people and agents read the same way. */
export function ParticipantName({ kind, name }: { kind: 'user' | 'agent'; name: string }) {
  return <span className="chat-participant-name"><span className="chat-name-text">{name}</span>{kind === 'agent' && <AiTag />}</span>;
}

export function LockIcon({ size = 15 }: { size?: number }) {
  return <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
    <rect x="5" y="11" width="14" height="9" rx="2" /><path d="M8 11V8a4 4 0 0 1 8 0v3" />
  </svg>;
}
