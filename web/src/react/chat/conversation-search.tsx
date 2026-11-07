import { useRef, useState } from 'react';
import { navigate } from '../../navigation';
import { workspaceUrl } from '../../workspace-context';
import { Dialog } from '../ui';
import { MessageResults } from './message-search';
import type { Conversation, Directory } from './model';

/** Search this conversation's messages; a result opens it here, in its thread if it has one. */
export function ConversationSearch({ chat, title, directory, close }: { chat: Conversation; title: string; directory: Directory; close: () => void }) {
  const [query, setQuery] = useState('');
  const input = useRef<HTMLInputElement>(null);
  const open = (id: string, message: string) => {
    close();
    navigate(workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(id)}&message=${encodeURIComponent(message)}`));
  };
  return <Dialog title={`Search in ${title}`} close={close} initialFocus={input}>
    <label className="ui-field">Words in messages
      <input ref={input} type="search" value={query} onChange={event => setQuery(event.target.value)} placeholder="Search this conversation…" />
    </label>
    <MessageResults query={query} chat={chat.id} chats={[chat]} directory={directory} open={open} />
  </Dialog>;
}
