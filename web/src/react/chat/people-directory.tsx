import { useState } from 'react';
import { workspaceSession, workspaceUrl } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { ParticipantFace, ParticipantName } from './conversation-face';
import { existingDirect, useDirectConversation, type DirectTarget } from './direct-conversation';
import type { Conversation, Directory } from './model';
import type { Pin, Preferences } from './organization-model';
import { samePin } from './organization-model';
import { useShowProfile } from './profile-card';

type Filter = 'all' | 'user' | 'agent';

/** Everyone in the workspace and every agent the viewer can use, in one list. */
export function PeopleDirectory({ directory, chats, preferences, togglePin, close, opened }: {
  directory: Directory; chats: Conversation[]; preferences?: Preferences; togglePin: (pin: Pin) => void;
  close: () => void; opened: (id: string) => void;
}) {
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<Filter>('all');
  const direct = useDirectConversation(chats, opened);
  const showProfile = useShowProfile();
  const me = workspaceSession?.user?.id;
  const everyone: DirectTarget[] = [
    ...directory.users.map(user => ({ ...user, kind: 'user' as const })),
    ...directory.agents.map(agent => ({ ...agent, kind: 'agent' as const })),
  ].sort((a, b) => a.name.localeCompare(b.name));
  const text = query.trim().toLowerCase();
  const shown = everyone.filter(item => (filter === 'all' || item.kind === filter)
    && `${item.name} ${item.title ?? ''} ${item.role ?? ''} ${item.email ?? ''}`.toLowerCase().includes(text));
  const count = (kind: Filter) => kind === 'all' ? everyone.length : everyone.filter(item => item.kind === kind).length;
  const pinTarget = (target: DirectTarget) => void direct.run(async () => {
    togglePin({ kind: 'chat', id: await direct.ensure(target) });
  });
  return <Dialog title="People & agents" busy={direct.busy} status={direct.status} close={close}
    footer={directory.role === 'Owner' ? <a className="ui-button" href={workspaceUrl('/?view=settings&scope=workspace&section=people')}>Invite people</a> : undefined}>
    <div className="people-directory">
      <input type="search" aria-label="Search people and agents" placeholder="Search by name or job" value={query} onChange={event => setQuery(event.target.value)} autoFocus />
      <div className="people-directory-filters" role="group" aria-label="Show">
        {([['all', 'All'], ['user', 'People'], ['agent', 'Agents']] as const).map(([value, label]) =>
          <Button key={value} aria-pressed={filter === value} onClick={() => setFilter(value)}>{label} <small>{count(value)}</small></Button>)}
      </div>
      <ul aria-label="People and agents">{shown.map(item => {
        const conversation = existingDirect(chats, item, me);
        const pinned = conversation && preferences?.pins.some(pin => samePin(pin, { kind: 'chat', id: conversation.id }));
        const self = item.kind === 'user' && item.id === me;
        const detail = item.kind === 'agent' ? [item.role, item.manager && `managed by ${item.manager}`].filter(Boolean).join(' · ') : item.title || (item.role === 'Owner' ? 'Owner' : '');
        return <li key={`${item.kind}:${item.id}`}>
          <button type="button" className="people-directory-who" onClick={() => showProfile({ kind: item.kind, id: item.id })} aria-label={`View profile of ${item.name}`}>
            <ParticipantFace kind={item.kind} who={item} />
            <span><ParticipantName kind={item.kind} name={self ? `${item.name} (you)` : item.name} /><small>{detail}</small></span>
          </button>
          {!self && preferences?.available && <Button className="ui-icon chat-pin-action" aria-pressed={Boolean(pinned)} aria-label={`${pinned ? 'Unpin' : 'Pin'} ${item.name}`} title={pinned ? 'Unpin' : 'Pin to Chat'} disabled={direct.busy} onClick={() => pinTarget(item)}>{pinned ? '★' : '☆'}</Button>}
          {!self && <Button disabled={direct.busy} onClick={() => void direct.choose(item)}>Message</Button>}
        </li>;
      })}</ul>
      {!shown.length && <p className="chat-help" role="status">Nobody matches “{query}”.</p>}
      <p className="chat-help">Everyone in this workspace can see this list.</p>
    </div>
  </Dialog>;
}
