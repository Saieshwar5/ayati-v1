import { messageHtml } from '../../markdown';
import '../../message-content.css';
import { MessageReceipt, useSeen } from './receipt';
import { workspaceSession } from '../../workspace-context';
import type { Directory, Message } from './model';
import { ApprovalCard } from '../approvals/approval-card';
import { ReplyProgress } from './reply-progress';
import { QuestionCards } from '../questions/card';
import { useResults } from '../results/provider';
import { ResponsePreview } from '../results/preview';
import { MessageFiles } from '../results/files';
export function ChatMessage({ message, directory, replies, openThread, compact = false, focused = false }: { focused?: boolean; compact?: boolean; message: Message; directory: Directory; replies?: number; openThread?: () => void }) {
  const openResult = useResults();
  const name = message.kind === 'agent' ? directory.agents.find(agent => agent.id === message.author)?.name ?? message.name ?? 'Agent' : message.name ?? 'Former member';
  const own = message.kind !== 'agent' && message.author === workspaceSession?.user?.id;
  const element = useSeen(message, own);
  const text = message.state === 'queued' ? message.progress?.text || message.content : message.content;
  return <article ref={element} data-message-id={message.id} className={`chat-message${own ? ' chat-message-own' : ''}${compact ? ' chat-message-compact' : ''}${focused ? ' chat-message-focused' : ''}`} aria-label={own ? 'Message from you' : `Message from ${name}`}>
    {!own && <span className={`chat-avatar ${message.kind}`}>{message.kind === 'agent' ? '✧' : name.slice(0, 1).toUpperCase()}</span>}
    <div>{!own && <header><strong>{own ? 'You' : name}</strong>{message.kind === 'agent' && <span className="chat-agent-label">Agent</span>}<time dateTime={new Date(message.at * 1000).toISOString()}>{new Date(message.at * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time></header>}
      {text && !message.approval && (message.kind === 'agent' && openResult && !message.response_unavailable ? <ResponsePreview id={message.id} text={text} reference={message.response} open={id => openResult({ kind: 'response', id })} /> : <div className="message-markdown" dangerouslySetInnerHTML={{ __html: messageHtml(text) }} />)}<ReplyProgress message={message} />
      {message.approval && <ApprovalCard reference={message.approval} />}
      {message.questions && <QuestionCards questions={message.questions} />}
      {message.attachments?.length ? <MessageFiles files={message.attachments} /> : null}
      {own && <div className="chat-message-meta"><time dateTime={new Date(message.at * 1000).toISOString()}>{new Date(message.at * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time><MessageReceipt message={message} /></div>}
    </div>
    {openThread && <button type="button" className={replies ? 'chat-thread-link chat-thread-count' : 'chat-thread-action'} aria-label={replies ? `${replies} ${replies === 1 ? 'reply' : 'replies'}` : 'Reply in thread'} title="Reply in thread" onClick={openThread}>{replies ? `${replies} ${replies === 1 ? 'reply' : 'replies'}` : '↩'}</button>}
  </article>;
}
