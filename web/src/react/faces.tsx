// Round pictures for people and agents. People show their photo or initials;
// agents show an uploaded image or a robot bundled with the app. An "AI" tag
// next to an agent's name is what tells the two apart.
import { workspaceId } from '../workspace-context';
import './faces.css';

export type PersonIdentity = { id: string; name: string; photo?: string | null };
export type AgentIdentity = { id: string; name: string; avatar?: string | null };

const ROBOTS = 24;
const uploaded = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const bundled = /^robot-(0[1-9]|1\d|2[0-4])$/;

function stableNumber(text: string): number {
  let value = 0;
  for (const char of text) value = (value * 31 + char.codePointAt(0)!) >>> 0;
  return value;
}

/** The robot an agent gets until someone picks a picture for it. */
export function defaultRobot(agentId: string): string {
  return `robot-${String(stableNumber(agentId) % ROBOTS + 1).padStart(2, '0')}`;
}

export function agentPicture(agent: AgentIdentity): string {
  const avatar = agent.avatar ?? '';
  if (bundled.test(avatar)) return `/agent-avatars/${avatar}.svg`;
  if (uploaded.test(avatar)) return `/api/identity/workspaces/${workspaceId}/agent-profiles/${agent.id}/avatar/${avatar}`;
  // Earlier symbol choices (spark, sun…) also show a robot now.
  return `/agent-avatars/${defaultRobot(agent.id)}.svg`;
}

export function personPhoto(person: PersonIdentity): string | undefined {
  return person.photo ? `/api/identity/workspaces/${workspaceId}/members/${person.id}/photo/${person.photo}` : undefined;
}

export function initials(name: string): string {
  return name.split(/[\s@._-]+/).filter(Boolean).slice(0, 2).map(part => Array.from(part)[0]).join('').toUpperCase() || '?';
}

export function PersonFace({ person, className = 'chat-avatar' }: { person: PersonIdentity; className?: string }) {
  const photo = personPhoto(person);
  return <span className={`${className} face person face-tone-${stableNumber(person.id) % 6}`} aria-hidden="true">
    {photo ? <img alt="" src={photo} loading="lazy" /> : initials(person.name)}
  </span>;
}

export function AgentFace({ agent, className = 'chat-avatar' }: { agent: AgentIdentity; className?: string }) {
  return <span className={`${className} face agent`} aria-hidden="true"><img alt="" src={agentPicture(agent)} loading="lazy" /></span>;
}

export function AiTag() {
  return <span className="ai-tag" title="AI agent">AI</span>;
}
