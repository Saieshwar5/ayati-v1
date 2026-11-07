import { useEffect, useId, useRef, useState } from 'react';
import { workspaceSession } from '../../workspace-context';
import { useMutation } from '../requests';
import { Button } from '../ui';
import { chatRequest, type Conversation } from './model';

/**
 * Leave #channel, the last row of Details (D245 screen 6), confirmed in place.
 * A private channel keeps someone who can manage it: its creator or a workspace
 * admin in it, as the server enforces. A public channel stays readable to
 * everyone in the workspace, so leaving it takes you off its people list.
 */
export function ChannelLeave({ chat, changed, left }: { chat: Conversation; changed: () => Promise<void>; left: () => void }) {
  const me = workspaceSession?.user?.id;
  const [asking, setAsking] = useState(false);
  const mutation = useMutation();
  const id = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const restore = useRef(false);
  useEffect(() => {
    if (asking) heading.current?.focus();
    else if (restore.current) { restore.current = false; trigger.current?.focus(); }
  }, [asking]);
  if (!me || !chat.users.includes(me)) return null;
  const open = chat.visibility === 'public';
  const managers = chat.managers ?? [];
  const last = !open && managers.includes(me) && managers.every(manager => manager === me);
  const cancel = () => { restore.current = true; mutation.setStatus(''); setAsking(false); };
  const confirm = () => void mutation.run(async () => {
    await chatRequest(`/chats/${chat.id}/leave`, {});
    if (open) { setAsking(false); await changed(); } else left();
  });
  return <section className="chat-details-section chat-details-leave">
    {asking ? <div className="chat-leave-review" role="group" aria-labelledby={id} aria-busy={mutation.busy}
      onKeyDown={event => { if (event.key === 'Escape' && !mutation.busy) { event.preventDefault(); event.stopPropagation(); cancel(); } }}>
      <h3 id={id} ref={heading} tabIndex={-1}>Leave #{chat.name}?</h3>
      <p>{open
        ? `#${chat.name} is public, so it stays in your list and you can still read it. You leave its people list.`
        : `You stop seeing #${chat.name}, its history and files, and your unfinished requests there stop. Someone who manages it can add you again.`}</p>
      {last && <p className="chat-help">Nobody else in #{chat.name} could manage it. Add a workspace admin to it before you leave.</p>}
      {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
      <div className="chat-archive-review-actions">
        <Button primary className="chat-archive-confirm" disabled={mutation.busy || last} onClick={confirm}>{mutation.busy ? 'Leaving…' : 'Leave channel'}</Button>
        <Button disabled={mutation.busy} onClick={cancel}>Cancel</Button>
      </div>
    </div> : <button ref={trigger} type="button" className="chat-details-leave-button" onClick={() => setAsking(true)}>
      <LeaveIcon />Leave #{chat.name}
    </button>}
  </section>;
}

function LeaveIcon() {
  return <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><path d="M10 4H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h4M15 8l4 4-4 4M19 12H9" /></svg>;
}
