/** The two choices every automation shares (schema 115): what a run remembers
 * of earlier runs, and where its results go besides Activity. Schedules and
 * app events use the same controls and the same words. */
export type Memory = 'fresh' | 'remember' | 'subject';

export const memoryLabels: Record<Memory, { title: string; detail: string }> = {
  fresh: { title: 'Fresh each run', detail: 'Sees nothing from earlier runs. Best for one-off events like a new email.' },
  remember: { title: 'Remember earlier runs', detail: 'Sees what this automation did before. Best for daily or weekly reports.' },
  subject: { title: 'Per subject', detail: 'Sees only earlier runs about the same subject: one student, patient or order. Best when your system serves many people.' },
};

export function MemoryChoice({ value, change, name, subject = false }: { value: Memory; change: (value: Memory) => void; name: string; subject?: boolean }) {
  const choices: Memory[] = subject ? ['fresh', 'remember', 'subject'] : ['fresh', 'remember'];
  return <fieldset className="routine-results"><legend>Memory</legend>
    {choices.map(choice => <label key={choice} className="routine-choice">
      <input type="radio" name={`${name}-memory`} checked={value === choice} onChange={() => change(choice)} />
      <span><strong>{memoryLabels[choice].title}</strong><small>{memoryLabels[choice].detail}</small></span>
    </label>)}
    <p className="ui-help">Runs never read anyone’s conversations, whichever you choose.</p>
  </fieldset>;
}

type Place = { id: string; name: string };

/** Results always go to Activity; posting one short summary is optional. */
export function ResultsChoice({ post, setPost, chat, setChat, conversations, more, name }: {
  post: boolean; setPost: (post: boolean) => void; chat: string; setChat: (chat: string) => void;
  conversations: Place[]; more?: boolean; name: string;
}) {
  return <fieldset className="routine-results"><legend>Where results go</legend>
    <label className="routine-choice"><input type="radio" name={`${name}-results`} checked={!post} onChange={() => setPost(false)} />
      <span><strong>Activity only</strong><small>Results, approvals and questions appear in Activity. No conversation gets a message.</small></span></label>
    <label className="routine-choice"><input type="radio" name={`${name}-results`} checked={post} disabled={!conversations.length} onChange={() => setPost(true)} />
      <span><strong>Activity, and post the result to</strong><small>One short message with a link to the run. The steps, approvals and questions stay in Activity.</small></span></label>
    {post && <label className="ui-field">Conversation<select aria-label="Conversation" value={chat} onChange={event => setChat(event.target.value)}>
      {conversations.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>}
    {!conversations.length && <p className="ui-help">To post results, first add this agent to a conversation you may use.</p>}
    {post && more && <p className="ui-help">Only the first 100 eligible conversations are listed.</p>}
  </fieldset>;
}
