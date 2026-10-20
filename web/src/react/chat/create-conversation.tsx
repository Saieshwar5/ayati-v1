import { useRef, useState } from 'react';
import { workspaceSession } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { chatRequest, type Directory } from './model';

export function CreateConversation({ directory, mode, close, created, privateAgent }: {
  directory: Directory; mode: 'channel' | 'direct'; close: () => void;
  created: (id: string) => void; privateAgent: (id: string) => void;
}) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<string[]>([]);
  const mutation = useMutation();
  const id = useRef(crypto.randomUUID());
  const users = directory.users.filter(user => user.id !== workspaceSession?.user?.id);
  const choices = [...users.map(user => ({ ...user, kind: 'user' })), ...directory.agents.map(agent => ({ ...agent, kind: 'agent' }))];
  const create = (members: string[], title: string) => mutation.run(async () => {
    const users = members.filter(key => key.startsWith('user:')).map(key => key.slice(5));
    const agents = members.filter(key => key.startsWith('agent:')).map(key => key.slice(6));
    const result = await chatRequest<{ id: string }>('/chats', { id: id.current, kind: mode, name: title, description, users, agents });
    created(result.id);
  });
  return <Dialog title={mode === 'channel' ? 'Create channel' : 'New message'} busy={mutation.busy} status={mutation.status} close={close}>
    <form className="chat-create" onSubmit={event => { event.preventDefault(); if (selected.length) void create(selected, name); }}>
      {mode === 'channel' && <><label>Channel name<input autoFocus required maxLength={100} value={name} onChange={e => setName(e.target.value)} placeholder="e.g. inventory" /></label>
        <label>Purpose <span>(optional)</span><input maxLength={300} value={description} onChange={e => setDescription(e.target.value)} placeholder="Stock, suppliers and incoming deliveries" /></label>
        <p className="chat-help">Private channel · only invited members can read messages. You are included.</p></>}
      <label>{mode === 'channel' ? 'Add people or agents' : 'Find a person or agent'}<input type="search" value={query} onChange={e => setQuery(e.target.value)} placeholder="Search workspace…" /></label>
      <div className="chat-choice-list">{choices.filter(item => item.name.toLowerCase().includes(query.toLowerCase())).map(item => {
        const key = `${item.kind}:${item.id}`;
        return mode === 'channel' ? <label className="chat-choice" key={key}><input type="checkbox" checked={selected.includes(key)} onChange={e => setSelected(values => e.target.checked ? [...values, key] : values.filter(value => value !== key))} /><span>{item.name}</span>{item.kind === 'agent' && <small>Agent</small>}</label>
          : <button type="button" className="chat-contact" key={key} onClick={() => item.kind === 'agent' && directory.role === 'Owner' ? privateAgent(item.id) : void create([key], item.name)}><span className={`chat-avatar ${item.kind}`}>{item.kind === 'agent' ? '✧' : item.name.slice(0, 1).toUpperCase()}</span><span>{item.name}</span>{item.kind === 'agent' && <small>Agent</small>}</button>;
      })}</div>
      {!choices.length && <p className="chat-help">Invite a workspace member or create an agent to start a conversation.</p>}
      {mode === 'channel' && <Button primary type="submit" disabled={!name.trim() || !selected.length || mutation.busy}>Create channel</Button>}
    </form>
  </Dialog>;
}
