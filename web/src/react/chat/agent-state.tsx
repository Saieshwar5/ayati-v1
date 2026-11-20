import { Button } from '../ui';
import './agent-state.css';
import { useMutation } from '../requests';
import { pauseAgent } from '../activity/model';
import type { Message, Participant } from './model';

/**
 * Whether an agent can take a message now: the agent's counterpart of a person's
 * presence. People's green dot means "here now"; agents are always here, so their
 * dot means "will pick up your message now". From the chat directory (paused,
 * needs_owner, all_paused). Only the state is shown: what an agent is working on
 * for others stays in Activity for its owners and admins.
 */
export type AgentState = 'ready' | 'paused' | 'needs_owner' | 'all_paused';

export function agentState(agent: Participant): AgentState {
  if (agent.all_paused) return 'all_paused';
  if (agent.needs_owner) return 'needs_owner';
  if (agent.paused) return 'paused';
  return 'ready';
}

export const takesMessages = (agent: Participant) => agentState(agent) === 'ready';

/** The words for a state, as the header and profile show them. */
export function agentStateText(agent: Participant): { label: string; detail: string } {
  switch (agentState(agent)) {
    case 'all_paused': return { label: 'Paused', detail: 'An admin paused all agents' };
    case 'needs_owner': return { label: 'Paused', detail: 'Its owner left · an admin can give it a new one' };
    case 'paused': return { label: 'Paused', detail: agent.yours ? 'You can resume it' : agent.manager ? `${agent.manager} can resume it` : 'Its owner can resume it' };
    default: return { label: 'Ready', detail: agent.role ?? '' };
  }
}

/** On the edge of an agent's face, where a person's online dot sits. Hollow when paused. */
export function AgentDot({ agent, working = false }: { agent: Participant; working?: boolean }) {
  const state = agentState(agent);
  return <span className={`presence-dot agent-dot agent-dot-${working && state === 'ready' ? 'working' : state === 'ready' ? 'ready' : 'paused'}`} aria-hidden="true" />;
}

/** What an agent is doing for you in this conversation, from its replies still in progress. */
export function workingForYou(messages: Message[], agent: string): string | undefined {
  const reply = [...messages].reverse().find(message => message.kind === 'agent' && message.author === agent && message.state === 'queued');
  if (!reply) return undefined;
  switch (reply.progress?.state) {
    case 'approval': return 'Waiting for approval';
    case 'input': return 'Waiting for an answer';
    case 'handoff': return 'Waiting for another agent';
    case undefined: return 'Starting your request…';
    default: return 'Working on your request…';
  }
}

/**
 * In place of the message box while a direct message's agent cannot take messages,
 * so nobody writes something the server will refuse. Its owners and admins resume
 * it here; the change reaches every open Chat page ('agents'). A draft stays saved.
 */
export function PausedBar({ agent, admin }: { agent: Participant; admin: boolean }) {
  const mutation = useMutation();
  const state = agentState(agent);
  const canResume = state === 'paused' && (agent.yours || admin);
  return <div className="chat-paused-bar" role="status">
    <span>{state === 'all_paused' ? 'All agents are paused, so ' : ''}{agent.name} {state === 'all_paused' ? 'takes' : 'is paused and takes'} no new messages</span>
    {canResume && <Button disabled={mutation.busy} onClick={() => void mutation.run(async () => { await pauseAgent(agent.id, false); })}>Resume {agent.name}</Button>}
    {mutation.status && <small className="chat-error">{mutation.status}</small>}
  </div>;
}
