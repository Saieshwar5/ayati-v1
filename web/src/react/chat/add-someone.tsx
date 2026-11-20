import { useEffect, useRef } from 'react';
import { Button } from '../ui';
import { CreateAgentTile, InviteTile } from './chat-start';
import type { PendingInvite, StartFacts } from './start-model';

/**
 * Opens under a section's ＋ when nobody could be added yet, instead of a form
 * that cannot be saved: Invite people (admins) or Create agent. From Direct
 * messages Invite comes first, since you came to message a person.
 * Escape or Close returns focus to the ＋ that opened it.
 */
export function AddSomeone({ mode, facts, invites, close }: {
  mode: 'channel' | 'direct'; facts: StartFacts; invites: PendingInvite[]; close: (restoreFocus: boolean) => void;
}) {
  const card = useRef<HTMLDivElement>(null);
  const latest = useRef(close); latest.current = close;
  useEffect(() => {
    card.current?.querySelector<HTMLElement>('a')?.focus();
    const away = (event: PointerEvent) => {
      const target = event.target as Element;
      if (!card.current?.contains(target) && !target.closest?.('[data-add-someone]')) latest.current(false);
    };
    document.addEventListener('pointerdown', away);
    return () => document.removeEventListener('pointerdown', away);
  }, []);
  const title = mode === 'channel' ? 'Add someone first' : 'Nobody to message yet';
  return <div ref={card} className="chat-add-someone" role="dialog" aria-label={title}
    onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); close(true); } }}>
    <header><h3>{title}</h3><Button className="ui-icon" aria-label="Close" onClick={() => close(true)}>×</Button></header>
    <div className={`chat-start-tiles${facts.admin ? '' : ' chat-start-single'}`}>
      {facts.admin && <InviteTile invites={invites} primary={mode === 'direct'} />}
      <CreateAgentTile />
    </div>
  </div>;
}
