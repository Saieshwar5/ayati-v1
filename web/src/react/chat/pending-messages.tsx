import { messageHtml } from '../../markdown';
import { useSyncExternalStore } from 'react';
import { outboxSnapshot, retryMessage, subscribeOutbox } from './outbox';
export function PendingMessages({ chat, thread }: { chat: string; thread?: string }) {
  const entries = useSyncExternalStore(subscribeOutbox, outboxSnapshot);
  return <>{entries.filter(item => item.chat === chat && item.payload.thread === thread).map(item => <article className="chat-message chat-message-own" key={item.key} aria-label="Your pending message">
    <div><div className="message-markdown" dangerouslySetInnerHTML={{ __html:messageHtml(item.payload.content) }} />
      <small role="status">{({ waiting: '◷ Waiting to send', sending: 'Sending…', checking: '◷ Checking delivery…', failed: '⚠ Not sent', paused: '⚠ Sending paused', sent: '✓ Sent · synchronizing'  })[item.state]}</small>
      {item.error && <p className="chat-send-error">{item.error}</p>}
      {item.payload.attachments.length > 0 && <small>{item.payload.attachments.length} uploaded attachment(s)</small>}
      {['failed', 'paused'].includes(item.state) && <button className="chat-thread-link" onClick={() => void retryMessage(item).catch(() => {})}>Retry this message</button>}
    </div>
  </article>)}</>;
}
