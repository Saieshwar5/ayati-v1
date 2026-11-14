// Your own connection shows only while it is down (D245, designs/chat-and-account
// screen 2 note 5): a thin banner under the conversation header, never a
// permanent "Connected" label. After a gap it says what happened once
// (`live-notice.ts`): how much was caught up, that the screen reloaded after a
// long absence, or that a newer Cuckoo needs a reload.
import { useEffect, useSyncExternalStore } from 'react';
import { Button } from '../ui';
import { connectionProblem, reconnectNow } from './live';
import { clearNotice, liveNotice, subscribeNotice, type LiveNotice } from './live-notice';

/** How long "Back online" stays before it fades. */
export const BACK_ONLINE_MS = 4000;

const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? '' : 's'}`;

function Notice({ notice }: { notice: LiveNotice }) {
  useEffect(() => {
    if (notice.kind !== 'back') return;
    const timer = window.setTimeout(() => clearNotice('back'), BACK_ONLINE_MS);
    return () => window.clearTimeout(timer);
  }, [notice]);
  if (notice.kind === 'back') return <div className="chat-connection-banner back" role="status">
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="M5 12l4 4 10-10" /></svg>
    <span><b>Back online.</b>{notice.messages > 0 && ` Caught up on ${plural(notice.messages, 'message')}.`}</span>
  </div>;
  if (notice.kind === 'reloaded') return <div className="chat-connection-banner reloaded" role="status">
    <span><b>You were away for a while.</b> Showing the latest messages.</span>
    <Button onClick={() => clearNotice('reloaded')}>Dismiss</Button>
  </div>;
  // Drafts are kept by the composer and unsent messages by the outbox, so a reload loses nothing.
  return <div className="chat-connection-banner updated" role="status">
    <span><b>Cuckoo was updated.</b> Reload to keep chatting. Your drafts are kept.</span>
    <Button onClick={() => window.location.reload()}>Reload</Button>
  </div>;
}

export function ConnectionBanner({ status }: { status: string }) {
  const notice = useSyncExternalStore(subscribeNotice, liveNotice);
  const problem = connectionProblem(status);
  if (!problem) return notice ? <Notice notice={notice} /> : null;
  return <div className={`chat-connection-banner ${problem}`} role="status">
    {problem === 'offline' ? <>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><path d="M2 8.5a15 15 0 0 1 20 0M5.5 12a10 10 0 0 1 13 0M9 15.5a5 5 0 0 1 6 0M12 19h.01M3 3l18 18" /></svg>
      <span>You’re offline. Messages you send will go out when you’re back.</span>
      <Button onClick={reconnectNow}>Retry</Button>
    </> : <>
      <svg className="chat-connection-spinner" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><path d="M20 12a8 8 0 1 1-2.34-5.66M20 4v4h-4" /></svg>
      <span><b>Reconnecting…</b> You can keep writing; messages send when you’re back.</span>
    </>}
  </div>;
}
