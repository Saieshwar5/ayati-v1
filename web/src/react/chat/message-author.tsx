import { workspaceSession } from '../../workspace-context';
import { AgentFace, PersonFace } from '../faces';
import type { Directory, Message } from './model';

/** Who wrote a message, as the conversation shows them: the agent or person when they are
 * still in the workspace, the name to show either way, their face, and whether it is you. */
export function messageAuthor(message: Message, directory: Directory) {
  const byAgent = message.kind === 'agent';
  const agent = byAgent ? directory.agents.find(item => item.id === message.author) : undefined;
  const person = byAgent ? undefined : directory.users.find(user => user.id === message.author);
  const name = byAgent ? agent?.name ?? message.name ?? 'Agent' : message.name ?? 'Former member';
  const own = !byAgent && message.author === workspaceSession?.user?.id;
  const face = agent ? <AgentFace agent={agent} /> : byAgent ? <AgentFace agent={{ id: message.author, name }} />
    : person ? <PersonFace person={person} /> : <PersonFace person={{ id: message.author, name }} />;
  return { agent, person, name, own, face };
}
