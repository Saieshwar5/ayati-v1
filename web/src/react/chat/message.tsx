import { messageHtml } from '../../markdown';
import '../../message-content.css';
import { MessageReceipt, useSeen } from './receipt';
import { workspaceSession } from '../../workspace-context';
import type { Directory, Message } from './model';
import { ApprovalCard } from '../approvals/approval-card';
import { ReplyProgress } from './reply-progress';
import { QuestionCards } from '../questions/card';
import { useResults, useResultSource, useResultReading } from '../results/provider';
import { InlineResponse } from '../results/inline-response';
import { HandoffCard, CoordinationView } from './handoffs';
import { MessageFiles } from '../results/files';
import { AgentFace, AiTag, PersonFace } from '../faces';
import { useShowProfile } from './profile-card';
import { useState } from 'react';
import { DeleteMessage, EditMessage, MessageToolbar, ReactionBar, useReactions } from './message-actions';
export function ChatMessage({ message, directory, replies, openThread, compact = false, focused = false }: { focused?: boolean; compact?: boolean; message: Message; directory: Directory; replies?: number; openThread?: () => void }) {
  const openResult = useResults();
  const source = useResultSource();
  const reading = useResultReading();
  const name = message.kind === 'agent' ? directory.agents.find(agent => agent.id === message.author)?.name ?? message.name ?? 'Agent' : message.name ?? 'Former member';
  const own = message.kind !== 'agent' && message.author === workspaceSession?.user?.id;
  const showProfile = useShowProfile();
  const agent = message.kind === 'agent' ? directory.agents.find(item => item.id === message.author) : undefined;
  const person = message.kind === 'agent' ? undefined : directory.users.find(user => user.id === message.author);
  const face = agent ? <AgentFace agent={agent} /> : message.kind === 'agent' ? <AgentFace agent={{ id: message.author, name }} />
    : person ? <PersonFace person={person} /> : <PersonFace person={{ id: message.author, name }} />;
  const profile = (agent || person) && (() => showProfile({ kind: agent ? 'agent' : 'user', id: message.author }));
  const element = useSeen(message, own);
  const text = message.state === 'queued' ? message.progress?.text || message.content : message.content;
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const reactions = useReactions(message);
  const deleted = message.state === 'deleted';
  const time = <time dateTime={new Date(message.at * 1000).toISOString()}>{new Date(message.at * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time>;
  const edited = message.edited_at && !deleted ? <span className="chat-message-edited">edited</span> : null;
  // Tapping a message on a touch screen focuses it and reveals its actions.
  return <article ref={element} tabIndex={-1} data-message-id={message.id} className={`chat-message${own ? ' chat-message-own' : ''}${compact ? ' chat-message-compact' : ''}${focused ? ' chat-message-focused' : ''}`} aria-label={own ? 'Message from you' : `Message from ${name}`}>
    {!own && (profile ? <button type="button" className="chat-message-face" aria-label={`View profile of ${name}`} onClick={profile}>{face}</button> : face)}
    <div>{!own && <header>{profile ? <button type="button" className="chat-message-author" onClick={profile}>{name}</button> : <strong>{name}</strong>}{message.kind === 'agent' && <AiTag />}{person?.title && <span className="chat-message-title">{person.title}</span>}{time}</header>}
      {deleted ? <p className="chat-message-deleted">Message deleted</p> : editing ? <EditMessage message={message} done={() => setEditing(false)} /> : <>
      {text && !message.approval && (message.kind === 'agent' && !message.response_unavailable && !message.handoff ? <InlineResponse key={message.id} id={message.id} text={text} reference={message.response} source={source} reading={reading} openReader={openResult ? () => openResult({ kind: 'response', id: message.id }) : undefined} /> : <div className="message-markdown" dangerouslySetInnerHTML={{ __html: messageHtml(text) }} />)}<ReplyProgress message={message} />
      {message.handoff && <HandoffCard handoff={message.handoff}/>}
      <CoordinationView data={message.coordination}/>
      {message.approval && <ApprovalCard reference={message.approval} />}
      {message.questions && <QuestionCards questions={message.questions} />}
      {message.attachments?.length ? <MessageFiles files={message.attachments} /> : null}
      {!own && edited}
      <ReactionBar message={message} reactions={reactions} />
      </>}
      {own && <div className="chat-message-meta">{edited}{time}<MessageReceipt message={message} /></div>}
    </div>
    {replies ? <button type="button" className="chat-thread-link chat-thread-count" aria-label={`${replies} ${replies === 1 ? 'reply' : 'replies'}`} title="Open thread" onClick={openThread}>{`${replies} ${replies === 1 ? 'reply' : 'replies'}`}</button> : null}
    {(message.chat || openThread) && !deleted && !editing && <MessageToolbar message={message} own={own} reactions={reactions} openThread={openThread} edit={() => setEditing(true)} remove={() => setDeleting(true)} />}
    {deleting && <DeleteMessage message={message} close={() => setDeleting(false)} />}
  </article>;
}
