import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { PersonFace } from '../faces';
import { Icon } from '../chat/panel-actions';
import { workspaceSession } from '../../workspace-context';
import { ownerNames, type Agent } from './model';
import { AgentAvatar, agentDotState } from './avatar';
import { jobOf } from './agent-roster';

export type AgentAction = 'pause' | 'identity' | 'owners';

/**
 * The agent page header (designs/agents-profile, 2026-11-21): its face with the
 * state dot, its name, and one line that says its state, its job and whose it
 * is. Message is offered only to someone who may talk to it; Pause and Resume
 * are buttons for its owners and admins, and ⋯ holds its name and owners.
 */
export function AgentWorkspaceHeader({ agent, busy, back, openChat, choose }: {
  agent: Agent; busy: boolean; back: () => void; openChat: () => void; choose: (action: AgentAction) => void;
}) {
  const state = agentDotState(agent);
  const editable = Boolean(agent.editable), needsOwner = Boolean(agent.needs_owner);
  const paused = state === 'paused';
  const me = workspaceSession?.user?.id;
  const owners = agent.owners ?? (agent.owner_id && agent.owner_name ? [{ id: agent.owner_id, name: agent.owner_name }] : []);
  const named = owners.length ? ownerNames(owners.map(owner => owner.id === me ? { ...owner, name: 'You' } : owner)) : agent.owner_name ?? '';
  // Someone who cannot resume it is told who can; for its owners the button says so.
  const detail = needsOwner ? 'Its last owner left the workspace'
    : paused && !editable ? `${named.replace(/^You\b/, 'you') || 'Its owner'} can resume it`
      : state === 'working' ? `${agent.pending} ${agent.pending === 1 ? 'request' : 'requests'}` : '';
  const resume = paused && editable && !needsOwner;
  return <header className="agent-workspace-header">
    <Button className="agent-roster-back" onClick={back}>‹ Agents</Button>
    <div className="agent-identity-row"><AgentAvatar agent={agent} dot />
      <div className="agent-identity-title">
        <h2>{agent.name}</h2>
        <p className="agent-state-line">
          <strong>{paused ? 'Paused' : state === 'working' ? 'Working' : 'Ready'}</strong>
          {[detail, jobOf(agent)].filter(Boolean).map(part => <span key={part}> · {part}</span>)}
          {/* An assistant's line already names its person. */}
          {!agent.assistant && !needsOwner && named && <span className="agent-state-owner"> · <span className="visually-hidden">owned by </span>
            {owners.slice(0, 3).map(owner => <PersonFace key={owner.id} person={owner} className="chat-avatar agent-owner-face" />)}{named}</span>}
        </p>
      </div>
      <div className="agent-header-actions">
        {resume && <Button primary disabled={busy} onClick={() => choose('pause')}><Icon name="play" />Resume</Button>}
        {agent.can_talk !== false && <Button primary={!resume} disabled={busy} onClick={openChat}><Icon name="message" />Message</Button>}
        {editable && !paused && <Button disabled={busy} onClick={() => choose('pause')}><Icon name="pause" />Pause</Button>}
        {editable && <AgentMenu name={agent.name} needsOwner={needsOwner} assistant={Boolean(agent.assistant)} choose={choose} />}
      </div>
    </div>
  </header>;
}

/** ⋯ actions for its owners and admins: its name and picture, and its owners. */
function AgentMenu({ name, needsOwner, assistant, choose }: { name: string; needsOwner: boolean; assistant: boolean; choose: (action: AgentAction) => void }) {
  const [open, setOpen] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLButtonElement>('button')?.focus();
    const outside = (event: MouseEvent) => { if (!menu.current?.contains(event.target as Node) && event.target !== toggle.current) setOpen(false); };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, [open]);
  // Focus returns to ⋯ first, so a dialog that opens hands it back there when it closes.
  const pick = (action: AgentAction) => { setOpen(false); toggle.current?.focus(); choose(action); };
  return <span className="agent-menu" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false); }}>
    <Button ref={toggle} className="ui-icon" aria-label={`More actions for ${name}`} aria-expanded={open} aria-haspopup="menu" onClick={() => setOpen(!open)}>⋯</Button>
    {open && <div ref={menu} className="chat-message-menu" role="menu" aria-label={`More actions for ${name}`}
      onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); setOpen(false); toggle.current?.focus(); } }}>
      <button type="button" role="menuitem" onClick={() => pick('identity')}>Name, picture and role…</button>
      {!assistant && <button type="button" role="menuitem" onClick={() => pick('owners')}>{needsOwner ? 'Give it an owner…' : 'Owners…'}</button>}
    </div>}
  </span>;
}
