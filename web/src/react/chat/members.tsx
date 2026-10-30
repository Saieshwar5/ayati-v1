import { useState } from 'react';
import { workspaceSession } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { chatRequest, type Conversation, type Directory } from './model';
export function ChannelMembers({ chat, directory }: { chat: Conversation; directory: Directory }) {
  const [members, setMembers] = useState(chat.users); const mutation = useMutation();
  return <form className="company-form" onSubmit={e => { e.preventDefault(); void mutation.run(async () => { await chatRequest(`/chats/${chat.id}/members`, { users: members }); mutation.setStatus('Membership saved.'); }); }}>
    {directory.users.map(user => <label key={user.id}><input type="checkbox" checked={members.includes(user.id)} disabled={mutation.busy || user.id === workspaceSession?.user?.id} onChange={e => setMembers(e.target.checked ? [...members, user.id] : members.filter(id => id !== user.id))} />{user.name}</label>)}
    <Button type="submit" disabled={mutation.busy}>Save channel members</Button><p role="status">{mutation.status}</p>
  </form>;
}
/** Channel managers choose which workspace agents work in the channel. */
export function ChannelAgents({ chat, directory }: { chat: Conversation; directory: Directory }) {
  const [agents, setAgents] = useState(chat.agents); const mutation = useMutation();
  const available = directory.agents.filter(agent => agent.scope !== 'private');
  if (!chat.can_archive || !available.length) return null;
  return <form className="company-form" onSubmit={e => { e.preventDefault(); void mutation.run(async () => { await chatRequest(`/chats/${chat.id}/agents`, { agents }); mutation.setStatus('Agents saved.'); }); }}>
    <h3>Agents in this channel</h3>
    {available.map(agent => <label key={agent.id}><input type="checkbox" checked={agents.includes(agent.id)} disabled={mutation.busy} onChange={e => setAgents(e.target.checked ? [...agents, agent.id] : agents.filter(id => id !== agent.id))} />{agent.name} <small>{agent.role}</small></label>)}
    <p className="chat-help">Agents act only when someone in the channel asks them.</p>
    <Button type="submit" disabled={mutation.busy}>Save agents</Button><p role="status">{mutation.status}</p>
  </form>;
}
export function ChannelSettings({ chat, directory }: { chat: Conversation; directory: Directory }) {
  const [name, setName] = useState(chat.name);
  const [visibility, setVisibility] = useState<string>(chat.visibility ?? 'private');
  const [announcements, setAnnouncements] = useState(chat.announcements ?? false);
  const mutation = useMutation();
  if (directory.role !== 'Owner') return null;
  return <section><h3>Channel access</h3><p>Making a channel public shares its existing history and published files with every workspace member.</p>
    <form className="company-form" onSubmit={e => { e.preventDefault(); void mutation.run(async () => { await chatRequest(`/chats/${chat.id}/settings`, { name, visibility, announcements }); mutation.setStatus('Channel settings saved.'); }); }}>
      <label>Channel name<input required maxLength={100} value={name} onChange={e => setName(e.target.value)} /></label>
      <label>Visibility<select value={visibility} onChange={e => setVisibility(e.target.value)}><option value="private">Private members only</option><option value="public">All workspace members</option></select></label>
      <label><input type="checkbox" checked={announcements} onChange={e => setAnnouncements(e.target.checked)} />Only owners and admins can post</label>
      <Button type="submit" disabled={mutation.busy}>Save channel access</Button>
    </form><p role="status">{mutation.status}</p>
  </section>;
}
