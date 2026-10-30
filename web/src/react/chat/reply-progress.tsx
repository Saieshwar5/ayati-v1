import { useMutation } from '../requests';
import { chatRequest, type Message } from './model';

/** Cancelling a running request, offered to the requester in the message menu. */
export function useCancelReply(message: Message) {
  const mutation = useMutation();
  const available = Boolean(message.can_cancel && message.chat && message.state === 'queued');
  const cancel = () => void mutation.run(async () => {
    await chatRequest(`/chats/${message.chat}/messages/${message.id}/cancel`, {});
  });
  return { ...mutation, available, cancel };
}

/** States that wait for a person stay visible; all other work reads as typing. */
function waitingLabel(message: Message) {
  switch (message.progress?.state) {
    case 'handoff': return 'Waiting for another agent';
    case 'approval': return 'Waiting for approval';
    case 'input': return 'Waiting for an answer';
    case 'attention': return 'Needs attention · External outcome not yet confirmed';
    default: return undefined;
  }
}

export function ReplyProgress({ message, name, cancel }: {
  message: Message; name: string; cancel: ReturnType<typeof useCancelReply>;
}) {
  if (['sent', 'complete'].includes(message.state)) return null;
  const waiting = message.state === 'queued' ? waitingLabel(message) : message.state;
  return <div className="chat-reply-progress">
    {waiting ? <small role="status">{waiting}</small> : <span className="chat-typing" role="status">
      <span className="chat-typing-dots" aria-hidden="true"><i /><i /><i /></span><small>{name} is typing…</small>
    </span>}
    {cancel.status && <small role="alert">{cancel.status}</small>}
  </div>;
}
