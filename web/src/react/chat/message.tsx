import { messageHtml } from '../../markdown';
import '../../message-content.css';
import { MessageReceipt, useSeen } from './receipt';
import { workspaceSession } from '../../workspace-context';
import { filesOnlyContent, type Directory, type Message } from './model';
import { ApprovalCard } from '../approvals/approval-card';
import { FollowUpLine, TaskChip } from './task-chip';
import { RoutineCard } from './routine-proposal';
import { ReplyProgress, useCancelReply } from './reply-progress';
import { QuotedMessage } from './quoted-message';
import { QuestionCards } from '../questions/card';
import { useResults, useResultSource, useResultReading } from '../results/provider';
import { InlineResponse } from '../results/inline-response';
import { HandoffCard, CoordinationView } from './handoffs';
import { MessageFiles } from '../results/files';
import { AgentFace, AiTag, PersonFace } from '../faces';
import { useShowProfile } from './profile-card';
import { useState } from 'react';
import { DeleteMessage, EditMessage, MessageToolbar, ReactionBar, useReactions } from './message-actions';
import { RequestDetails } from '../activity/request-details';
import { SaveAnswer } from './save-answer';
import { doorLabel } from './doors';
import { ThreadLine, type threadSummary } from './thread-line';
import { ForwardedBlock, ForwardMessage, forwardedOnly } from './forward-message';
import { AssistantMark } from './assistant-threads';
import { TakeOver } from './assistant-takeover';
export function ChatMessage({ message, directory, thread, openThread, reply, quoted, handOver, place = '', compact = false, focused = false }: { focused?: boolean;
  /** Your assistant's name, when this message can hand the chat over to it (schema 136). */
  handOver?: string; compact?: boolean; message: Message; directory: Directory; thread?: ReturnType<typeof threadSummary>; openThread?: () => void; reply?: () => void; quoted?: { name: string; text: string };
  /** Where this message is, as a forward of it says (`Direct message`, `#orders`). */
  place?: string }) {
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
  const placeholder = message.content === filesOnlyContent && Boolean(message.attachments?.length);
  // A forward without words of its own shows only what it forwards (schema 125).
  const bareForward = Boolean(message.forwarded) && message.content === forwardedOnly;
  const text = message.state === 'queued' ? message.progress?.text || message.content : placeholder || bareForward ? '' : message.content;
  // Just a voice message: its time and ticks can share the transcript line.
  const voiceOnly = own && placeholder && message.attachments?.length === 1 && message.attachments[0].mime.startsWith('audio/');
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [inspecting, setInspecting] = useState(false);
  const [keeping, setKeeping] = useState(false);
  const [forwarding, setForwarding] = useState(false);
  const [takingOver, setTakingOver] = useState(false);
  const writer = own && message.assisted?.by === 'assistant' ? directory.agents.find(item => item.id === message.assisted?.agent)?.name ?? 'your assistant' : undefined;
  const reactions = useReactions(message);
  const cancel = useCancelReply(message);
  const deleted = message.state === 'deleted';
  const time = <time dateTime={new Date(message.at * 1000).toISOString()}>{new Date(message.at * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</time>;
  const edited = message.edited_at && !deleted ? <span className="chat-message-edited">edited</span> : null;
  const via = doorLabel(message.door);
  const door = via ? <span className="chat-message-door">{via}</span> : null;
  if (message.follow_up) return <FollowUpLine element={element} id={message.id} task={message.follow_up} time={time} />;
  // Tapping a message on a touch screen focuses it and reveals its actions.
  return <article ref={element} tabIndex={-1} data-message-id={message.id} className={`chat-message${own ? ' chat-message-own' : ''}${compact ? ' chat-message-compact' : ''}${focused ? ' chat-message-focused' : ''}`} aria-label={own ? 'Message from you' : `Message from ${name}`}>
    {!own && (profile ? <button type="button" className="chat-message-face" aria-label={`View profile of ${name}`} onClick={profile}>{face}</button> : face)}
    <div className={voiceOnly ? 'chat-voice-bubble' : undefined}>{!own && <header>{profile ? <button type="button" className="chat-message-author" onClick={profile}>{name}</button> : <strong>{name}</strong>}{message.kind === 'agent' && <AiTag />}{person?.title && <span className="chat-message-title">{person.title}</span>}{time}{door}</header>}
      {deleted ? <p className="chat-message-deleted">Message deleted</p> : editing ? <EditMessage message={message} done={() => setEditing(false)} /> : <>
      {message.reply_to && quoted && <QuotedMessage id={message.reply_to} {...quoted} />}
      {message.forwarded && <ForwardedBlock forwarded={message.forwarded} />}
      {(text || message.response?.page) && !message.approval && (message.kind === 'agent' && !message.response_unavailable && !message.handoff ? <InlineResponse key={message.id} id={message.id} text={text} reference={message.response} source={source} reading={reading} openReader={openResult ? () => openResult({ kind: 'response', id: message.id }) : undefined} openPage={openResult ? () => openResult({ kind: 'page', id: message.id }) : undefined} /> : <div className="message-markdown" dangerouslySetInnerHTML={{ __html: messageHtml(text) }} />)}<ReplyProgress message={message} name={name} cancel={cancel} />
      {message.handoff && <HandoffCard handoff={message.handoff} onRequest />}
      <CoordinationView data={message.coordination} shownIn={message.thread ?? null} />
      {message.approval && <ApprovalCard reference={message.approval} />}
      {message.task && <TaskChip task={message.task} />}
      {message.routines?.map(routine => <RoutineCard key={routine.id} routine={routine} agent={name} />)}
      {message.questions && <QuestionCards questions={message.questions} />}
      {message.attachments?.length ? <MessageFiles files={message.attachments} /> : null}
      {!own && edited}
      <ReactionBar message={message} reactions={reactions} directory={directory} />
      </>}
      {own && <div className="chat-message-meta">{writer && <span className="chat-assisted" title={`Written by ${writer} · ${message.assisted?.title}. Only you and workspace admins can see this.`}><AssistantMark size={12} />{writer}</span>}{door}{edited}{time}<MessageReceipt message={message} /></div>}
    </div>
    {thread && <ThreadLine summary={thread} directory={directory} open={openThread} />}
    {(message.chat || openThread) && !deleted && !editing && <MessageToolbar message={message} own={own} reactions={reactions} cancel={cancel} reply={reply} openThread={openThread} edit={() => setEditing(true)} remove={() => setDeleting(true)} keep={() => setKeeping(true)} forward={() => setForwarding(true)}
      takeOver={handOver ? { label: `Let ${handOver} take over from here`, open: () => setTakingOver(true) } : undefined}
      details={message.kind === 'agent' && message.chat && (message.update_of || message.has_task !== false) ? () => setInspecting(true) : undefined} />}
    {deleting && <DeleteMessage message={message} close={() => setDeleting(false)} />}
    {takingOver && handOver && <TakeOver message={message} name={name} assistantName={handOver} close={() => setTakingOver(false)} />}
    {forwarding && <ForwardMessage message={message} name={name} place={place} directory={directory} close={() => setForwarding(false)} />}
    {keeping && <SaveAnswer message={message} close={() => setKeeping(false)} />}
    {inspecting && <RequestDetails reply={message.update_of ?? message.id} close={() => setInspecting(false)} />}
  </article>;
}
