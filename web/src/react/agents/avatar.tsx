import { symbols, base, type Agent } from './model';

export function AgentAvatar({ agent }: { agent: Agent }) {
  const avatar = agent.settings.avatar;
  return <span className="ui-avatar agent-profile-avatar">
    {symbols[avatar] ?? <img alt="" src={`/api/identity${base}/${agent.id}/avatar/${avatar}`} />}
  </span>;
}
