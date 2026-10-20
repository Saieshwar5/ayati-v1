import type { Directory, Message } from './model';
export function ChatMessage({ message, directory, replies, openThread }: { message: Message; directory: Directory; replies?: number; openThread?: () => void }) {
  const name = message.kind === 'agent' ? directory.agents.find(agent => agent.id === message.author)?.name ?? 'Agent' : message.name ?? 'Former member';
  return <article className="chat-message">
    <span className={`chat-avatar ${message.kind}`}>{message.kind === 'agent' ? '✧' : name.slice(0, 1).toUpperCase()}</span>
    <div><header><strong>{name}</strong>{message.kind === 'agent' && <span className="chat-agent-label">Agent</span>}<time dateTime={new Date(message.at * 1000).toISOString()}>{new Date(message.at * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time></header>
      <p>{message.content || 'Preparing a reply…'}</p>{!['sent', 'complete'].includes(message.state) && <small role="status">{message.state === 'queued' ? 'Working on a reply…' : message.state}</small>}
      {openThread && <button type="button" className="chat-thread-link" onClick={openThread}>{replies ? `${replies} ${replies === 1 ? 'reply' : 'replies'}` : 'Reply in thread'}</button>}
    </div>
  </article>;
}
