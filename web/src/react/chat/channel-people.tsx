import { useEffect, useId, useRef, useState } from 'react';
import { workspaceSession } from '../../workspace-context';
import { useMutation } from '../requests';
import { Button } from '../ui';
import { ChannelAdd } from './channel-add';
import { ParticipantFace, ParticipantName } from './conversation-face';
import { chatRequest, type Conversation, type Directory, type Participant } from './model';
import type { ProfileTarget } from './profile-card';

type Member = { kind: 'user' | 'agent'; who: Participant; known: boolean };
type Kind = Member['kind'];
type Actions = { message: (target: ProfileTarget & { name: string }) => void; viewProfile: (target: ProfileTarget) => void };

/**
 * One list of the channel's people and agents (D245, screen 5). Managers — the
 * creator or a workspace admin, as the server enforces — add and remove them.
 */
export function ChannelPeople({ chat, directory, changed, message, viewProfile }: Actions & {
  chat: Conversation; directory: Directory; changed: () => Promise<void>;
}) {
  const me = workspaceSession?.user?.id;
  const manages = Boolean(chat.can_archive);
  const heading = useId();
  const [query, setQuery] = useState('');
  const [adding, setAdding] = useState<Kind>();
  const [notice, setNotice] = useState('');
  const mutation = useMutation();
  const search = useRef<HTMLInputElement>(null);
  const addPeople = useRef<HTMLButtonElement>(null), addAgent = useRef<HTMLButtonElement>(null);
  // Each save sends the whole list, so a second change builds on the first
  // until the refreshed conversation arrives with both.
  const server = JSON.stringify([chat.users, chat.agents]);
  const [written, setWritten] = useState<{ server: string; users: string[]; agents: string[] }>();
  const users = written?.server === server ? written.users : chat.users;
  const agents = written?.server === server ? written.agents : chat.agents;
  const save = (kind: Kind, next: string[], done: string) => mutation.run(async () => {
    setNotice('');
    if (kind === 'user') await chatRequest(`/chats/${chat.id}/members`, { users: next });
    else await chatRequest(`/chats/${chat.id}/agents`, { agents: next });
    setWritten({ server, users: kind === 'user' ? next : users, agents: kind === 'agent' ? next : agents });
    setNotice(done);
    void changed();
  });
  const add = (kind: Kind, who: Participant) => save(kind, [...(kind === 'user' ? users : agents), who.id], `Added ${who.name} to #${chat.name}.`);
  const remove = (member: Member) => void save(member.kind, (member.kind === 'user' ? users : agents).filter(id => id !== member.who.id),
    `Removed ${member.who.name} from #${chat.name}.`).then(() => search.current?.focus());
  const stopAdding = () => { const kind = adding; setAdding(undefined); (kind === 'agent' ? addAgent : addPeople).current?.focus(); };

  const byName = (a: Member, b: Member) => a.who.name.localeCompare(b.who.name);
  const people: Member[] = users.map(id => known('user', id, directory.users.find(user => user.id === id)));
  const bots: Member[] = agents.map(id => known('agent', id, directory.agents.find(agent => agent.id === id)));
  const everyone = [...people.filter(member => member.who.id === me), ...bots.sort(byName), ...people.filter(member => member.who.id !== me).sort(byName)];
  const text = query.trim().toLowerCase();
  const shown = everyone.filter(({ who }) => `${who.name} ${who.title ?? ''} ${who.role ?? ''} ${who.email ?? ''}`.toLowerCase().includes(text));
  const counted = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;
  return <section className="chat-details-section" aria-labelledby={heading}>
    <header className="chat-details-section-header">
      <h3 id={heading}>People and agents</h3>
      <small>{counted(users.length, 'person', 'people')} · {counted(agents.length, 'agent', 'agents')}</small>
    </header>
    {!adding && <input ref={search} type="search" className="chat-details-search" aria-label={`Find people and agents in #${chat.name}`}
      placeholder={`Find in #${chat.name}`} value={query} onChange={event => setQuery(event.target.value)} />}
    {manages && <div className="chat-details-add">
      <Button ref={addPeople} aria-expanded={adding === 'user'} onClick={() => adding === 'user' ? stopAdding() : setAdding('user')}><AddPersonIcon />Add people</Button>
      <Button ref={addAgent} aria-expanded={adding === 'agent'} onClick={() => adding === 'agent' ? stopAdding() : setAdding('agent')}><AddAgentIcon />Add agent</Button>
    </div>}
    {adding ? <ChannelAdd key={adding} kind={adding} chat={chat} directory={directory} present={adding === 'user' ? users : agents}
      busy={mutation.busy} add={who => add(adding, who)} done={stopAdding} /> : <>
      {shown.length > 0 && <ul className="chat-details-members" aria-label={`People and agents in #${chat.name}`}>
        {shown.map(member => <MemberRow key={`${member.kind}:${member.who.id}`} member={member} chat={chat} me={me} manages={manages}
          manager={member.kind === 'user' && (member.who.role === 'Admin' || (member.who.id === me && manages))}
          busy={mutation.busy} message={message} viewProfile={viewProfile} remove={() => remove(member)} />)}
      </ul>}
      {!everyone.length && <p className="chat-help">Nobody has joined #{chat.name} yet.</p>}
      {everyone.length > 0 && !shown.length && <p className="chat-help" role="status">Nobody in #{chat.name} matches “{query.trim()}”.</p>}
    </>}
    {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
    <p className="ui-status" role="status">{notice}</p>
    <p className="chat-help">Agents act only when someone in the channel asks them.</p>
  </section>;
}

function known(kind: Kind, id: string, who: Participant | undefined): Member {
  return { kind, who: who ?? { id, name: kind === 'agent' ? 'Agent' : 'Former member' }, known: Boolean(who) };
}

function MemberRow({ member, chat, me, manages, manager, busy, message, viewProfile, remove }: Actions & {
  member: Member; chat: Conversation; me?: string; manages: boolean; manager: boolean; busy: boolean; remove: () => void;
}) {
  const { kind, who } = member;
  const self = kind === 'user' && who.id === me;
  const detail = kind === 'agent' ? [who.role, who.manager && `managed by ${who.manager}`].filter(Boolean).join(' · ') : who.title ?? '';
  const identity = <>
    {/* The presence dot will sit on this face with the later presence change. */}
    <ParticipantFace kind={kind} who={who} />
    <span><ParticipantName kind={kind} name={self ? `${who.name} (you)` : who.name} />{detail && <small>{detail}</small>}</span>
  </>;
  const items = [
    ...(member.known && !self ? [{ label: 'Message', run: () => message({ kind, id: who.id, name: who.name }) }] : []),
    ...(member.known ? [{ label: 'View profile', run: () => viewProfile({ kind, id: who.id }) }] : []),
    ...(manages && !self ? [{ label: `Remove from #${chat.name}`, danger: true, run: remove }] : []),
  ];
  return <li className="chat-details-member">
    {member.known ? <button type="button" className="chat-details-who" aria-label={`View profile of ${who.name}`} onClick={() => viewProfile({ kind, id: who.id })}>{identity}</button>
      : <span className="chat-details-who">{identity}</span>}
    {manager && <span className="ui-badge">Manager</span>}
    <MemberMenu name={who.name} busy={busy} items={items} />
  </li>;
}

/** A row's ⋯ menu: Message, View profile and, for managers, Remove. */
function MemberMenu({ name, busy, items }: { name: string; busy: boolean; items: { label: string; danger?: boolean; run: () => void }[] }) {
  const [open, setOpen] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    const outside = (event: MouseEvent) => { if (!menu.current?.contains(event.target as Node) && !toggle.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, [open]);
  if (!items.length) return null;
  const pick = (run: () => void) => { setOpen(false); toggle.current?.focus(); run(); };
  const move = (step: number) => {
    const buttons = [...(menu.current?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') ?? [])];
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    buttons[(index + step + buttons.length) % buttons.length]?.focus();
  };
  return <span className="chat-details-menu" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false); }}
    onKeyDown={event => {
      if (!open) return;
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); setOpen(false); toggle.current?.focus(); }
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); move(event.key === 'ArrowDown' ? 1 : -1); }
    }}>
    <Button ref={toggle} className="ui-icon" aria-label={`Actions for ${name}`} aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>⋯</Button>
    {open && <div ref={menu} className="chat-message-menu" role="menu" aria-label={`Actions for ${name}`}>
      {items.map(item => <button key={item.label} type="button" role="menuitem" className={item.danger ? 'danger' : undefined}
        disabled={item.danger && busy} onClick={() => pick(item.run)}>{item.label}</button>)}
    </div>}
  </span>;
}

function AddPersonIcon() {
  return <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><circle cx="9" cy="8" r="3.5" /><path d="M3 20a6 6 0 0 1 12 0M18 8v6M15 11h6" /></svg>;
}

function AddAgentIcon() {
  return <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><rect x="4" y="8" width="16" height="11" rx="3" /><path d="M12 4v4M9 13h.01M15 13h.01" /></svg>;
}
