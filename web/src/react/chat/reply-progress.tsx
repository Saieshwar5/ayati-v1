import { useMutation } from '../requests';
import { Button } from '../ui';
import { chatRequest, type Message } from './model';

export function ReplyProgress({ message }: { message: Message }) {
  const mutation = useMutation();
  if (['sent', 'complete'].includes(message.state)) return null;
  let label = message.state;
  if (message.state === 'queued') {
    switch (message.progress?.state) {
      case 'handoff': label = 'Waiting for another agent · Worker released'; break;
      case 'approval': label = 'Waiting for approval · Worker released'; break;
      case 'input': label = 'Waiting for an answer'; break;
      case 'resuming': label = 'Saved update · Waiting to continue…'; break;
      case 'attention': label = 'Needs attention · External outcome not yet confirmed'; break;
      case 'responding': label = 'Responding…'; break;
      case 'waiting': label = 'Waiting for a response…'; break;
      case 'interrupted': label = 'Reply interrupted. Waiting to retry…'; break;
      case 'starting': label = (message.progress.generation ?? 1) > 1 ? 'Retrying the reply…' : 'Preparing a reply…'; break;
      default: label = 'Waiting for a reply…';
    }
  }
  return <div className="chat-reply-progress">
    <small role="status">{label}</small>
    {message.can_cancel && message.chat && message.state === 'queued' && <Button disabled={mutation.busy} onClick={() => void mutation.run(async () => {
      await chatRequest(`/chats/${message.chat}/messages/${message.id}/cancel`, {});
    })}>Stop reply</Button>}
    {mutation.status && <small role="alert">{mutation.status}</small>}
  </div>;
}
