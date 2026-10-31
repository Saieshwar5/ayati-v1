import { useEffect, useRef, useState, type ReactNode } from 'react';
import { Button } from '../ui';

export type AgentAction = 'pause' | 'identity' | 'transfer';

/** The agent page header: picture, name, status, job title and owner, with
 * Open in Chat and the ⋯ menu (designs/agents-simplified 5). */
export function AgentWorkspaceHeader({ name, title, avatar, paused, owner, editable, busy, back, openChat, choose }: {
  name: string; title: string; avatar: ReactNode; paused: boolean; owner?: string; editable: boolean; busy: boolean;
  back: () => void; openChat: () => void; choose: (action: AgentAction) => void;
}) {
  const subtitle = [title, owner && `owned by ${owner}`].filter(Boolean).join(' · ');
  return <header className="agent-workspace-header">
    <Button className="agent-roster-back" onClick={back}>‹ Agents</Button>
    <div className="agent-identity-row">{avatar}
      <div className="agent-identity-title">
        <div className="agent-identity-name"><h2>{name}</h2><span className={`agent-availability${paused ? ' paused' : ''}`}>{paused ? 'Paused' : 'Available'}</span></div>
        {subtitle && <p>{subtitle}</p>}
      </div>
      <div className="agent-header-actions">
        <Button primary disabled={busy} onClick={openChat}>Open in Chat</Button>
        {editable && <AgentMenu name={name} paused={paused} choose={choose} />}
      </div>
    </div>
  </header>;
}

/** ⋯ actions for the owner and admins: pause or resume, rename, hand over. */
function AgentMenu({ name, paused, choose }: { name: string; paused: boolean; choose: (action: AgentAction) => void }) {
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
      <button type="button" role="menuitem" onClick={() => pick('pause')}>{paused ? 'Resume' : 'Pause'}</button>
      <button type="button" role="menuitem" onClick={() => pick('identity')}>Name and picture…</button>
      <button type="button" role="menuitem" onClick={() => pick('transfer')}>Transfer owner…</button>
    </div>}
  </span>;
}
