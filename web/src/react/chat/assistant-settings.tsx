import { agentSectionUrl } from '../agents/workspace-tabs';

/** In the header of your assistant's own chat: opens its page in Agents, where
 * what it knows about you, its rules, skills, apps and automations all live. */
export function AssistantSettingsLink({ agent, name }: { agent: string; name: string }) {
  return <a className="ui-button ui-icon chat-assistant-settings" href={agentSectionUrl(agent, 'Instructions')}
    aria-label={`${name} settings`} title={`${name} settings: what it knows about you, its rules, skills, apps and automations`}>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true"><path d="M4 7h9M17 7h3M4 17h3M11 17h9" /><circle cx="15" cy="7" r="2" /><circle cx="9" cy="17" r="2" /></svg>
  </a>;
}

/** The same shortcut for a team agent, shown to its owners and to admins, who can
 * change its instructions, skills, apps and who may use it. */
export function AgentSettingsLink({ agent, name }: { agent: string; name: string }) {
  return <a className="ui-button ui-icon chat-assistant-settings" href={agentSectionUrl(agent, 'Instructions')}
    aria-label={`${name} settings`} title={`${name} settings: instructions, skills, apps and who can use it`}>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true"><path d="M4 7h9M17 7h3M4 17h3M11 17h9" /><circle cx="15" cy="7" r="2" /><circle cx="9" cy="17" r="2" /></svg>
  </a>;
}
