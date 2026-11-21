import { AgentFace } from '../faces';
import type { Agent } from './model';
import '../chat/presence.css';
import '../chat/agent-state.css';

/** An agent's state as its dot shows it, the same three as in Chat: green takes
 * work now, purple is working, hollow is paused (or has no owner, so it is paused). */
export type AgentDotState = 'ready' | 'working' | 'paused';
export const agentDotState = (agent: Agent): AgentDotState =>
  agent.needs_owner || agent.settings.paused ? 'paused' : agent.pending ? 'working' : 'ready';

/** `dot` puts the agent's state on the edge of its face, where Chat shows it. */
export function AgentAvatar({ agent, dot = false }: { agent: Agent; dot?: boolean }) {
  return <AgentFace agent={{ id: agent.id, name: agent.name, avatar: agent.settings.avatar }} className="ui-avatar agent-profile-avatar"
    badge={dot ? <span className={`presence-dot agent-dot agent-dot-${agentDotState(agent)}`} aria-hidden="true" /> : undefined} />;
}
