import { useState } from 'react';
import { currentWorkspace } from '../../workspace-context';
import { chatRequest, type Directory } from '../chat/model';
import { PersonFace, initials } from '../faces';
import { useResource } from '../requests';
import { Button } from '../ui';
import { LeaveWorkspaceDialog } from './leave-workspace-dialog';
import { comingLater, labels, type Section } from './settings-sections';

const loadDirectory = () => chatRequest<Directory>('/chat-directory');
const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

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
    <div className="settings-workspace-card"><span className="settings-workspace-mark" aria-hidden="true">{initials(name)}</span>
      <div><strong>{name}</strong><p>You are a <span className="ui-badge">Member</span>
        {directory && ` · ${count(directory.users.length, 'person', 'people')} · ${count(directory.agents.length, 'agent', 'agents')}`}</p></div></div>
    <h3>Admins</h3><p>Ask them to invite people, add connections or change who can use an agent.</p>
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

const PLANNED: Partial<Record<Section, string>> = {
  usage: 'See what your agents and models cost: spend by day, the busiest agents and a table by model.',
  billing: 'Your plan, payment method and invoices. Cuckoo is free while early access lasts.',
};

/** Usage and Billing have their place in Settings before they are built. */
export function ComingLater({ section }: { section: Section }) {
  if (!comingLater.has(section)) return null;
  return <section className="settings-coming-later"><h2>{labels[section]} <span className="ui-badge settings-soon">Coming later</span></h2>
    <p>{PLANNED[section]}</p><p className="ui-help">This page is not built yet. Nothing here changes your workspace.</p></section>;
}
