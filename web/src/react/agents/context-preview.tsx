import { useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import type { Conversation } from '../chat/model';
import { useContextPreview } from './use-context-preview';
import { SavedContext, WorkingSummary } from './saved-context';

function EvidenceText({ text }: { text: string }) {
  const [expanded, expand] = useState(false);
  const long = text.length > 240;
  return <><p>{long && !expanded ? text.slice(0, 240) + '…' : text}</p>
    {long && <Button aria-expanded={expanded} onClick={() => expand(!expanded)}>{expanded ? 'Show less' : 'Read full text'}</Button>}
  </>;
}
function Timestamp({ seconds }: { seconds: number }) {
  const date = new Date(seconds * 1000);
  return <time dateTime={date.toISOString()}>{date.toLocaleString()}</time>;
}
export function ContextPreviewDialog({ agent, conversations, conversationError, close }: {
  agent: string; conversations: Conversation[]; conversationError: string; close: () => void;
}) {
  const [chat, select] = useState('');
  const first = useRef<HTMLSelectElement>(null);
  const { data, error, busy, refresh } = useContextPreview(agent, chat);
  return <Dialog drawer title="Conversation context preview" close={close} initialFocus={first}
    footer={<Button disabled={!chat || busy} onClick={refresh}>Refresh preview</Button>}>
    <div className="agent-context-preview">
      <p className="ui-help">See current eligible evidence and saved task context. Opening this view does not start new work.</p>
      <label className="ui-field">Conversation to preview
        <select ref={first} value={chat} onChange={event => select(event.target.value)}>
          <option value="">Choose a conversation…</option>
          {conversations.map(conversation => <option key={conversation.id} value={conversation.id}>{conversation.name}</option>)}
        </select>
      </label>
      {conversationError && <p role="alert">Conversation choices are unavailable: {conversationError}</p>}
      {!conversationError && !conversations.length && <p className="ui-help">No accessible conversations with this agent. Open Chat to start one.</p>}
      {error && <p role="alert">{error}</p>}
      {busy && !data && <p role="status">Loading current context…</p>}
      {data && <>
        <WorkingSummary working={data.working} />
        <SavedContext attempts={data.attempts} agent={agent} chat={chat} />
        <p className="ui-help">Previewed <Timestamp seconds={data.assembled_at} />. Channel history: {data.history_policy === 'since_join' ? 'only since this agent joined' : 'including before joining'}.</p>
        {(data.paused || data.archived) && <p className="ui-help">{data.paused ? 'This agent is paused. ' : ''}{data.archived ? 'This channel is archived. ' : ''}Previewing does not enable new work.</p>}
        <section aria-label="Eligible recent messages"><h3>Recent messages · {data.messages.length}</h3>
          <p className="ui-help">A bounded selection of up to {data.message_limit} messages. Source-dependent answers are excluded. Authorized older history can be retrieved during work.</p>
          {!data.messages.length && <p>No eligible recent messages.</p>}
          {data.messages.map(message => <article key={message.id}>
            <header><strong>{message.author.display_name || (message.author.kind === 'agent' ? 'Agent' : 'Person')}</strong>
              <small>{message.author.kind === 'agent' ? 'Agent' : 'Person'} · <Timestamp seconds={message.at} /></small></header>
            <EvidenceText text={message.content} />
          </article>)}
        </section>
        <section aria-label="Eligible approved notes"><h3>Approved notes · {data.notes.length}</h3>
          <p className="ui-help">Only notes that fit this conversation’s current context are shown. This may not include every saved note.</p>
          {!data.knowledge_enabled && <p className="ui-help">Approved knowledge is off for this agent.</p>}
          {data.knowledge_enabled && !data.notes.length && <p>No approved notes are eligible for this conversation.</p>}
          {data.notes.map(note => <article key={note.id}><strong>{note.title}</strong><EvidenceText text={note.content} /></article>)}
        </section>
      </>}
    </div>
  </Dialog>;
}
