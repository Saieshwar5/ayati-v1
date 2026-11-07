// Your own connection shows only while it is down (D245, designs/chat-and-account
// screen 2 note 5): a thin banner under the conversation header, never a
// permanent "Connected" label.
import { Button } from '../ui';
import { connectionProblem, reconnectNow } from './live';

export function ConnectionBanner({ status }: { status: string }) {
  const problem = connectionProblem(status);
  if (!problem) return null;
  return <div className={`chat-connection-banner ${problem}`} role="status">
    {problem === 'offline' ? <>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><path d="M2 8.5a15 15 0 0 1 20 0M5.5 12a10 10 0 0 1 13 0M9 15.5a5 5 0 0 1 6 0M12 19h.01M3 3l18 18" /></svg>
      <span>You’re offline. Messages you send will go out when you’re back.</span>
      <Button onClick={reconnectNow}>Retry</Button>
    </> : <>
      <svg className="chat-connection-spinner" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><path d="M20 12a8 8 0 1 1-2.34-5.66M20 4v4h-4" /></svg>
      <span>Reconnecting…</span>
    </>}
  </div>;
}
