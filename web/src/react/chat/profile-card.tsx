import { createContext, useContext, useState, type ReactNode } from 'react';
import { workspaceSession, workspaceUrl } from '../../workspace-context';
import { agentSectionUrl } from '../agents/workspace-tabs';
import { AgentFace, AiTag, initials, PersonFace } from '../faces';
import { useMutation } from '../requests';
import { chatRequest, type Conversation, type Directory, type Participant } from './model';
import { SidePanel } from './side-panel';
import { activeStatus, shownOnline, usePresence } from './presence';
import { clearsLabel, lastSeenLabel, PresenceDot, statusEmoji, statusLabel } from './presence-view';
import { AgentDot, agentState, agentStateText, takesMessages } from './agent-state';
import { pauseAgent } from '../activity/model';
import { ActionTile, ActionLink, Icon } from './panel-actions';
import './profile-card.css';

export type ProfileTarget = { kind: 'user' | 'agent'; id: string };

/** Opens the profile of whoever was clicked; provided once by the Chat screen. */
export const ShowProfile = createContext<(target: ProfileTarget) => void>(() => {});
export const useShowProfile = () => useContext(ShowProfile);

/** A person's or agent's profile in the side panel beside the conversation (D245, screen 2). */
export function ProfilePanel({ target, directory, chats, close, message, back }: {
  target: ProfileTarget; directory: Directory; chats: Conversation[]; close: () => void;
  message: (target: ProfileTarget & { name: string }) => void; back?: { label: string; go: () => void };
}) {
  const who = find(target, directory);
  const label = `${who?.name ?? 'Profile'} profile`;
  // The name under the face is the title people see; the heading keeps it for screen readers.
  return <SidePanel title={<span className="visually-hidden">{who?.name ?? 'Profile'}</span>} label={label} close={close} back={back}>
    <ProfileDetails target={target} directory={directory} chats={chats} message={message} />
  </SidePanel>;
}

const find = (target: ProfileTarget, directory: Directory) => target.kind === 'user'
  ? directory.users.find(user => user.id === target.id) : directory.agents.find(item => item.id === target.id);

/**
 * One profile for people and agents, wherever it opens (2026-11-20). Who first,
 * then what you can do, then a few facts. Only the actions change with where you
 * are: `message` is left out inside your direct message with them, which passes
 * its bell and Pin as `conversation` instead.
 */
export function ProfileDetails({ target, directory, chats, message, conversation }: {
  target: ProfileTarget; directory: Directory; chats: Conversation[];
  message?: (target: ProfileTarget & { name: string }) => void;
  /** Inside your direct message with them: its bell and Pin. */
  conversation?: ReactNode;
}) {
  const who = find(target, directory);
  const [adding, setAdding] = useState(false);
  if (!who) return <p className="chat-help">This person or agent is no longer in the workspace.</p>;
  const person = target.kind === 'user' ? who : undefined;
  const agent = target.kind === 'agent' ? who : undefined;
  const shared = chats.filter(chat => chat.kind !== 'direct' && !chat.archived
    && (target.kind === 'user' ? chat.users.includes(target.id) : chat.agents.includes(target.id)));
  const self = target.kind === 'user' && target.id === workspaceSession?.user?.id;
  const admin = directory.role === 'Admin';
  const manages = Boolean(agent && (agent.yours || admin));
  const channels = agent && agent.audience !== 'only_me' && !agent.assistant
    ? chats.filter(chat => chat.kind !== 'direct' && !chat.archived && chat.can_archive && !chat.agents.includes(agent.id)) : [];
  return <div className="profile-card">
    <div className="profile-card-identity">
      {person ? <PersonFace person={person} className="profile-card-face" badge={<PresenceDot id={person.id} />} />
        : <AgentFace agent={agent!} className="profile-card-face" badge={<AgentDot agent={agent!} />} />}
      <h3>{who.name}{agent && <AiTag />}</h3>
      <p>{person ? person.title || (person.role === 'Admin' ? 'Workspace admin' : 'Member') : agent!.role}</p>
      {person && <ProfilePresence id={person.id} name={person.name} />}
      {agent && <AgentStateLine agent={agent} />}
    </div>
    <div className="panel-actions" role="group" aria-label={`${who.name}: actions`}>
      {agent && manages && agentState(agent) === 'paused' && <ResumeTile agent={agent} />}
      {message && !self && <ActionTile icon={<Icon name="message" />} label="Message" onClick={() => message({ ...target, name: who.name })} />}
      {conversation}
      {channels.length > 0 && <ActionTile icon={<Icon name="plus" />} label="Add to channel" expanded={adding} onClick={() => setAdding(open => !open)} />}
      {agent && manages && <ActionLink icon={<Icon name="settings" />} label="Settings" href={agentSectionUrl(agent.id, 'Instructions')} title={`${agent.name} settings`} />}
      {self && <ActionLink icon={<Icon name="edit" />} label="Edit profile" href={workspaceUrl('/?view=settings&section=profile')} />}
    </div>
    {adding && agent && <AddToChannel agent={agent} channels={channels} />}
    <ul className="profile-card-facts">
      {person?.about && <Fact icon="about" label="About">{person.about}</Fact>}
      {agent?.handles && <Fact icon="about" label="Handles">{agent.handles}</Fact>}
      {person?.email && <Fact icon="mail" label="Email"><EmailLine email={person.email} /></Fact>}
      {agent?.manager && <Fact icon="person" label={agent.manager.includes(',') ? 'Owners' : 'Owner'}><Owners agent={agent} directory={directory} /></Fact>}
      <Fact icon="hash" label={agent ? 'Works in' : self ? 'Your channels' : 'Channels together'}>
        {shared.length ? <span className="profile-card-chips">{shared.slice(0, 8).map(chat =>
          <a key={chat.id} href={workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(chat.id)}`)}># {chat.name}</a>)}
          {shared.length > 8 && <span>+{shared.length - 8} more</span>}</span> : <span className="profile-card-none">No channels yet</span>}
      </Fact>
    </ul>
  </div>;
}

/** One fact: an icon and its value; the label is for screen readers and on hover. */
function Fact({ icon, label, children }: { icon: Parameters<typeof Icon>[0]['name']; label: string; children: ReactNode }) {
  return <li className="profile-card-fact" title={label}><Icon name={icon} /><span className="visually-hidden">{label}: </span><div>{children}</div></li>;
}

function EmailLine({ email }: { email: string }) {
  const [copied, setCopied] = useState(false);
  return <span className="profile-card-email">
    <a href={`mailto:${email}`}>{email}</a>
    <button type="button" className="ui-icon profile-card-copy" aria-label={`Copy ${email}`} title="Copy"
      onClick={() => void navigator.clipboard?.writeText(email).then(() => setCopied(true), () => setCopied(false))}><Icon name="copy" /></button>
    <small role="status">{copied ? 'Copied' : ''}</small>
  </span>;
}

/** An agent's owners with their faces; one you can see opens their profile. */
function Owners({ agent, directory }: { agent: Participant; directory: Directory }) {
  const showProfile = useShowProfile();
  const me = workspaceSession?.user?.id;
  const names = (agent.manager ?? '').split(',').map(name => name.trim()).filter(Boolean);
  return <span className="profile-card-owners">{names.map(name => {
    const owner = directory.users.find(user => user.name === name);
    const face = owner ? <PersonFace person={owner} className="profile-card-mini" /> : <span className="profile-card-mini face" aria-hidden="true">{initials(name)}</span>;
    const label = owner?.id === me ? 'You' : name;
    return owner ? <button key={name} type="button" className="profile-card-owner" onClick={() => showProfile({ kind: 'user', id: owner.id })}
      aria-label={`View profile of ${name}`}>{face}{label}</button> : <span key={name} className="profile-card-owner">{face}{label}</span>;
  })}</span>;
}

function AgentStateLine({ agent }: { agent: Participant }) {
  const { label, detail } = agentStateText(agent);
  const ready = takesMessages(agent);
  return <p className="profile-card-presence"><span className={`agent-line-dot agent-dot-${ready ? 'ready' : 'paused'}`} aria-hidden="true" />{label}{!ready && <span className="profile-card-detail"> · {detail}</span>}</p>;
}

/** Its owners and admins resume a paused agent right here; the change reaches every open page. */
function ResumeTile({ agent }: { agent: Participant }) {
  const mutation = useMutation();
  return <>
    <ActionTile primary icon={<Icon name="play" />} label="Resume" aria-label={`Resume ${agent.name}`} disabled={mutation.busy}
      onClick={() => void mutation.run(async () => { await pauseAgent(agent.id, false); })} />
    {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
  </>;
}

/** "Online" or "Last seen …", then the status with when it clears and who set it (screen 2, note 4). */
function ProfilePresence({ id, name }: { id: string; name: string }) {
  const presence = usePresence(id);
  const status = activeStatus(presence);
  const online = shownOnline(presence);
  const self = id === workspaceSession?.user?.id;
  const ends = status ? clearsLabel(status) : '';
  return <>
    {online ? <p className="profile-card-presence"><span className="presence-dot inline" aria-hidden="true" />Online</p>
      : status?.kind !== 'away' && presence?.last_seen ? <p className="profile-card-presence">{lastSeenLabel(presence.last_seen)}</p> : null}
    {status && <div className="profile-card-status">
      <strong><span aria-hidden="true">{statusEmoji(status)}</span> {statusLabel(status)}</strong>
      <small>{[ends && ends[0].toUpperCase() + ends.slice(1), `set by ${self ? 'you' : name.split(' ')[0]}`].filter(Boolean).join(' · ')}</small>
    </div>}
  </>;
}

/** The channels you manage, one tap each; the agent joins at once (the server checks you may). */
function AddToChannel({ agent, channels }: { agent: Participant; channels: Conversation[] }) {
  const mutation = useMutation();
  const [added, setAdded] = useState<string[]>([]);
  return <div className="profile-card-add" role="group" aria-label={`Add ${agent.name} to a channel`}>
    {channels.map(chat => {
      const done = added.includes(chat.id);
      return <button key={chat.id} type="button" className="profile-card-add-row" disabled={mutation.busy || done}
        aria-label={done ? `${agent.name} added to #${chat.name}` : `Add ${agent.name} to #${chat.name}`}
        onClick={() => void mutation.run(async () => {
          await chatRequest(`/chats/${chat.id}/agents`, { agents: [...chat.agents, agent.id] });
          setAdded(previous => [...previous, chat.id]);
          mutation.setStatus(`Added to #${chat.name}.`);
        })}><span># {chat.name}</span><Icon name={done ? 'check' : 'plus'} /></button>;
    })}
    <p className="chat-help" role="status">{mutation.status}</p>
  </div>;
}
