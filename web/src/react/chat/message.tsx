import { workspaceId, workspaceSession } from '../../workspace-context';
import type { Directory, Message } from './model';
export function ChatMessage({ message, directory, replies, openThread }: { message: Message; directory: Directory; replies?: number; openThread?: () => void }) {
  const name = message.kind === 'agent' ? directory.agents.find(agent => agent.id === message.author)?.name ?? 'Agent' : message.name ?? 'Former member';
  const own = message.kind !== 'agent' && message.author === workspaceSession?.user?.id;
  return <article className={`chat-message${own ? ' chat-message-own' : ''}`} aria-label={own ? 'Message from you' : `Message from ${name}`}>
    <span className={`chat-avatar ${message.kind}`}>{message.kind === 'agent' ? '✧' : name.slice(0, 1).toUpperCase()}</span>
    <div><header><strong>{own ? 'You' : name}</strong>{message.kind === 'agent' && <span className="chat-agent-label">Agent</span>}<time dateTime={new Date(message.at * 1000).toISOString()}>{new Date(message.at * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time></header>
      <p>{message.content || 'Preparing a reply…'}</p>{!['sent', 'complete'].includes(message.state) && <small role="status">{message.state === 'queued' ? 'Working on a reply…' : message.state}</small>}
      {message.attachments?.map(file => {
        const url = `/api/identity/workspaces/${workspaceId}/objects/${file.id}`;
        return <div className="chat-file" key={file.id}>{file.mime.startsWith('image/') && <img loading="lazy" src={url} alt={file.name} />}{file.mime.startsWith('audio/') && <audio controls preload="none" src={url} aria-label={file.name} />}<a href={url} download={file.name}>{file.name}</a><small> · {Math.ceil(file.size / 1024)} KB</small></div>;
      })}
      {openThread && <button type="button" className="chat-thread-link" onClick={openThread}>{replies ? `${replies} ${replies === 1 ? 'reply' : 'replies'}` : 'Reply in thread'}</button>}
    </div>
  </article>;
}
