import { createContext, useContext, useState } from 'react';
import { workspaceSession } from '../../workspace-context';
import { AgentFace, AiTag, PersonFace } from '../faces';
import { useMutation } from '../requests';
import { Button, Dialog } from '../ui';
import { chatRequest, type Conversation, type Directory } from './model';

export type ProfileTarget = { kind: 'user' | 'agent'; id: string };

/** Opens the profile of whoever was clicked; provided once by the Chat screen. */
export const ShowProfile = createContext<(target: ProfileTarget) => void>(() => {});
export const useShowProfile = () => useContext(ShowProfile);

export function ProfileCard({ target, directory, chats, close, message }: {
  target: ProfileTarget; directory: Directory; chats: Conversation[]; close: () => void;
  message: (target: ProfileTarget & { name: string }) => void;
}) {
  const person = target.kind === 'user' ? directory.users.find(user => user.id === target.id) : undefined;
  const agent = target.kind === 'agent' ? directory.agents.find(item => item.id === target.id) : undefined;
  const who = person ?? agent;
  const shared = chats.filter(chat => chat.kind !== 'direct' && !chat.archived
    && (target.kind === 'user' ? chat.users.includes(target.id) : chat.agents.includes(target.id)));
  const self = target.kind === 'user' && target.id === workspaceSession?.user?.id;
  return <Dialog title={target.kind === 'agent' ? 'Agent profile' : 'Profile'} drawer close={close}>
    {!who ? <p className="chat-help">This person or agent is no longer in the workspace.</p> : <div className="profile-card">
      {person ? <PersonFace person={person} className="profile-card-face" /> : <AgentFace agent={agent!} className="profile-card-face" />}
      <div className="profile-card-identity">
        <h3>{who.name}{agent && <AiTag />}</h3>
        <p>{person ? person.title || (person.role === 'Admin' ? 'Workspace admin' : 'Member') : agent!.role}</p>
      </div>
      {!self && <div className="profile-card-actions"><Button primary onClick={() => message({ ...target, name: who.name })}>Message</Button></div>}
      {person?.about && <ProfileLine label="About">{person.about}</ProfileLine>}
      {person?.email && <ProfileLine label="Email">{person.email}</ProfileLine>}
      {agent?.manager && <ProfileLine label="Managed by">{agent.manager}</ProfileLine>}
      <ProfileLine label={agent ? 'Works in' : self ? 'Your channels' : 'Channels together'}>
        {shared.length ? <span className="profile-card-chips">{shared.slice(0, 8).map(chat => <span key={chat.id}># {chat.name}</span>)}{shared.length > 8 && <span>+{shared.length - 8} more</span>}</span> : 'No channels yet'}
      </ProfileLine>
      {agent && agent.audience !== 'only_me' && <AddToChannel agent={agent.id} chats={chats} />}
      {agent && <p className="chat-help">{agent.name} is an AI agent. It acts when someone asks it and shows its work in the conversation.</p>}
    </div>}
  </Dialog>;
}

function ProfileLine({ label, children }: { label: string; children: React.ReactNode }) {
  return <div className="profile-card-line"><span>{label}</span><div>{children}</div></div>;
}

/** Channel managers (creator or owner) can bring an agent into one of their channels. */
function AddToChannel({ agent, chats }: { agent: string; chats: Conversation[] }) {
  const options = chats.filter(chat => chat.kind !== 'direct' && !chat.archived && chat.can_archive && !chat.agents.includes(agent));
  const [choice, setChoice] = useState('');
  const mutation = useMutation();
  if (!options.length) return null;
  const chat = options.find(item => item.id === choice);
  return <form className="profile-card-add" onSubmit={event => { event.preventDefault(); if (!chat) return; void mutation.run(async () => {
    await chatRequest(`/chats/${chat.id}/agents`, { agents: [...chat.agents, agent] });
    setChoice('');
    mutation.setStatus(`Added to #${chat.name}.`);
  }); }}>
    <label>Add to a channel<select value={choice} disabled={mutation.busy} onChange={event => setChoice(event.target.value)}>
      <option value="">Choose a channel…</option>
      {options.map(item => <option key={item.id} value={item.id}>#{item.name}</option>)}
    </select></label>
    <Button type="submit" disabled={!chat || mutation.busy}>{mutation.busy ? 'Adding…' : 'Add'}</Button>
    <p className="chat-help" role="status">{mutation.status}</p>
  </form>;
}
