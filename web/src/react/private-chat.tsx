import { useEffect, useState } from 'react';
import { api } from '../types';
import type { Profile } from '../agents';
import { workspaceUrl } from '../workspace-context';
import { Dialog } from './ui';
import { LegacyView } from './legacy-view';
import '../agents.css';

type Entry = { profile: Profile; activity: { pending_approvals: number; pending_tasks: number } };
type Directory = { agents: Entry[]; creation_available: boolean; limit: number };

/** The contact list survives agent changes; only the conversation surface remounts. */
export function PrivateChat({ href }: { href: string }) {
  const agent = new URL(href).searchParams.get('agent') ?? 'local-agent';
  const [directory, setDirectory] = useState<Directory>();
  const [error, setError] = useState('');
  const [query, setQuery] = useState('');
  const [collapsed, setCollapsed] = useState(false);
  const [chooser, setChooser] = useState(false);
  useEffect(() => {
    let active = true, busy = false;
    const abort = new AbortController();
    async function refresh(force = false) {
      if (busy || (!force && document.hidden)) return;
      busy = true;
      try {
        const data = await api<Directory>('/agents', { signal: abort.signal }, 'local-agent');
        if (active) { setDirectory(data); setError(''); }
      } catch { if (active) setError('Agent list unavailable. Reconnecting…'); }
      finally { busy = false; }
    }
    void refresh(true);
    const timer = window.setInterval(() => void refresh(), 2500);
    return () => { active = false; abort.abort(); window.clearInterval(timer); };
  }, []);
  useEffect(() => { setChooser(false); }, [agent]);
  const contacts = <>
    <a className="files-button" href={workspaceUrl('/?view=chat')}>People &amp; group chats</a>
    <div className="agents-heading"><h2>Chat</h2><a className="files-button" href={workspaceUrl('/?view=agents')}>Manage</a></div>
    <label className="agent-search-label">Search agents<input id="agent-search" type="search" placeholder="Search agents…" value={query} onChange={event => setQuery(event.target.value)} /></label>
    <div id="agent-cards">{directory?.agents.filter(({ profile }) => profile.state !== 'archived' && `${profile.name} ${profile.role}`.toLowerCase().includes(query.toLowerCase())).map(({ profile, activity }) => {
      const name = profile.id === 'local-agent' && profile.name === 'Ayati' ? 'Relay' : profile.name;
      return <div key={profile.id} className={`agent-card${profile.id === agent ? ' selected' : ''}`} data-state={profile.state !== 'ready' ? profile.state : activity.pending_approvals ? 'waiting' : activity.pending_tasks ? 'working' : 'ready'} title={profile.state === 'ready' ? name : profile.detail}>
        <span className="agent-avatar" aria-hidden="true">{name.split(/\s+/).map(part => part[0]).slice(0, 2).join('').toUpperCase()}</span>
        <a className="agent-link" href={profile.state === 'ready' ? workspaceUrl(`/?agent=${encodeURIComponent(profile.id)}`) : undefined} aria-disabled={profile.state !== 'ready' || undefined} aria-current={profile.id === agent ? 'page' : undefined}>{name}</a>
        {!!activity.pending_approvals && <span className="agent-attention" aria-label={`${activity.pending_approvals} approvals need attention`}>{activity.pending_approvals}</span>}
      </div>;
    })}</div>
    <p id="agents-status" role="status">{error || (!directory ? 'Loading agents…' : !directory.creation_available ? 'Creating agents requires local VM setup.' : directory.agents.length >= directory.limit ? `Local limit: ${directory.limit} agents.` : '')}</p>
  </>;
  return <div className={`private-chat-layout${collapsed ? ' contacts-collapsed' : ''}`} onClick={event => {
    const id = (event.target as Element).closest('button')?.id;
    if (id === 'agent-sidebar-toggle') {
      const button = (event.target as Element).closest('button');
      button?.setAttribute('aria-expanded', String(collapsed));
      button?.setAttribute('aria-label', collapsed ? 'Hide agent list' : 'Show agent list');
      setCollapsed(value => !value);
    }
    if (id === 'agent-list-toggle') setChooser(true);
  }}>
    <aside className="agents-list persistent-contacts" aria-label="Agents" id="agents-sidebar">{contacts}</aside>
    <LegacyView key={href} page="chat" href={href} profile={directory?.agents.find(entry => entry.profile.id === agent)?.profile} />
    {chooser && <Dialog title="Choose agent" close={() => setChooser(false)}><div className="mobile-agent-contacts">{contacts}</div></Dialog>}
  </div>;
}
