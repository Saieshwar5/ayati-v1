import { AgentFace } from '../faces';
import type { Agent } from './model';

export function AgentAvatar({ agent }: { agent: Agent }) {
  return <AgentFace agent={{ id: agent.id, name: agent.name, avatar: agent.settings.avatar }} className="ui-avatar agent-profile-avatar" />;
}
