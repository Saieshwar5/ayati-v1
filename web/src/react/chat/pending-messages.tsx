import { messageHtml } from '../../markdown';
import { useSyncExternalStore } from 'react';
import { askedAgents, outboxSnapshot, retryMessage, subscribeOutbox, type PendingMessage } from './outbox';
import { filesOnlyContent } from './model';

/** One line per agent the message asks: each answers separately once the message is sent. */
function AskedAgents({ item, agentName }: { item: PendingMessage; agentName: (id: string) => string | undefined }) {
  const agents = askedAgents(item.payload);
  if (!agents.length) return null;
  const label = (name: string) => ['failed', 'paused'].includes(item.state) ? `${name} not asked yet` : item.state === 'sent' ? `Asked ${name}` : `Asking ${name}…`;
  return <ul className="chat-pending-agents" aria-label="Agents asked">
    {agents.map(id => <li key={id}><small>{label(agentName(id) ?? 'Agent')}</small></li>)}
  </ul>;
}

export function PendingMessages({ chat, thread, agentName = () => undefined }: { chat: string; thread?: string; agentName?: (id: string) => string | undefined }) {
  const entries = useSyncExternalStore(subscribeOutbox, outboxSnapshot);
  return <>{entries.filter(item => item.chat === chat && item.payload.thread === thread).map(item => <article className="chat-message chat-message-own" key={item.key} aria-label="Your pending message">
    <div>{!(item.payload.content === filesOnlyContent && item.payload.attachments.length) && <div className="message-markdown" dangerouslySetInnerHTML={{ __html:messageHtml(item.payload.content) }} />}
      <small role="status">{({ waiting: '◷ Waiting to send', sending: 'Sending…', checking: '◷ Checking delivery…', failed: '⚠ Not sent', paused: '⚠ Sending paused', sent: '✓ Sent · synchronizing'  })[item.state]}</small>
      <AskedAgents item={item} agentName={agentName} />
      {item.error && <p className="chat-send-error">{item.error}</p>}
      {item.payload.attachments.length > 0 && <small>{item.payload.attachments.length} uploaded attachment(s)</small>}
      {['failed', 'paused'].includes(item.state) && <button className="chat-thread-link" onClick={() => void retryMessage(item).catch(() => {})}>Retry this message</button>}
    </div>
  </article>)}</>;
}
