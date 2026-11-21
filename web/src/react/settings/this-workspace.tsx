import { useState, type ReactNode } from 'react';
import { currentWorkspace } from '../../workspace-context';
import { chatRequest, type Directory } from '../chat/model';
import { PersonFace, initials } from '../faces';
import { useResource } from '../requests';
import { Button } from '../ui';
import { LeaveWorkspaceDialog } from './leave-workspace-dialog';

const loadDirectory = () => chatRequest<Directory>('/chat-directory');
export const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** The workspace's tile, name and one line about it, at the top of its page. */
export function WorkspaceCard({ name, line }: { name: string; line: ReactNode }) {
  return <div className="settings-workspace-card"><span className="settings-workspace-mark" aria-hidden="true">{initials(name)}</span>
    <div><strong>{name}</strong><p>{line}</p></div></div>;
}

/** What a member can know about the workspace: its name, their role, who manages it and
 * what admins can see. Members can leave from here. */
export function ThisWorkspace() {
  const workspace = currentWorkspace();
  const name = workspace?.name ?? 'This workspace';
  const resource = useResource(loadDirectory);
  const [leaving, setLeaving] = useState(false);
  const directory = resource.data;
  const admins = directory?.users.filter(person => person.role === 'Admin') ?? [];
  return <section className="settings-this-workspace"><h2>This workspace</h2><p>About {name} and who manages it.</p>
    <WorkspaceCard name={name} line={<>You are a <span className="ui-badge">Member</span>
      {directory && ` · ${count(directory.users.length, 'person', 'people')} · ${count(directory.agents.length, 'agent', 'agents')}`}</>} />
    <h3>Admins</h3><p>Ask them to invite people, allow an app or change a rule.</p>
    {resource.error && <><p role="alert">{resource.error}</p><Button onClick={() => void resource.refresh().catch(() => {})}>Retry</Button></>}
    {!resource.data && !resource.error && <p role="status">Loading admins…</p>}
    <ul className="settings-admins">{admins.map(person => <li key={person.id}>
      <PersonFace person={{ id: person.id, name: person.name, photo: person.photo }} className="ui-avatar" />
      <span><strong>{person.name}</strong>{person.email && <small>{person.email}</small>}</span></li>)}</ul>
    <h3>What admins can see</h3>
    <p>Admins manage people, connections and settings, and read the Audit log. Like everyone else, they see public channels and the ones they’re in, never other people’s private channels or direct messages.</p>
    <div className="settings-leave"><div><h3>Leave workspace</h3><p>You will lose access to its channels, agents and files.</p></div>
      <Button className="danger" disabled={!directory} onClick={() => setLeaving(true)}>Leave workspace</Button></div>
    {leaving && directory && <LeaveWorkspaceDialog people={directory.users} close={() => setLeaving(false)} />}
  </section>;
}
