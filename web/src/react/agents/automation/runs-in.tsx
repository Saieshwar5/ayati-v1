/** Where a trigger runs (schema 151, the owner's one rule): in a conversation, as
 * the person who set it up, exactly like them asking the agent there. Choosing
 * none runs it in their direct chat with the agent. Every trigger form uses these
 * controls and words. */
type Place = { id: string; name: string; kind?: string };

/** The conversation a run happens in, for a review or a saved trigger. */
export const runsInText = (agentName: string, place?: string | null, direct = false) =>
  place && !direct ? place : `Your direct chat with ${agentName}`;

/** What a run does where it runs. */
export const runsInHelp = (agentName: string) =>
  `Each run happens there as you, like you asking ${agentName}: it reads that conversation, replies there once and says nothing when there is nothing to tell. Approvals and questions come to you.`;

export function RunsInChoice({ chat, setChat, conversations, agentName, more }: {
  chat: string; setChat: (chat: string) => void; conversations: Place[]; agentName: string; more?: boolean;
}) {
  // A direct chat listed here is the person's own with the agent: the first choice.
  const places = conversations.filter(place => place.kind !== 'direct');
  return <fieldset className="routine-results"><legend>Runs in</legend>
    <label className="ui-field">Conversation<select aria-label="Runs in" value={chat} onChange={event => setChat(event.target.value)}>
      <option value="">{runsInText(agentName)}</option>
      {places.map(place => <option key={place.id} value={place.id}>{place.name}</option>)}
    </select></label>
    <p className="ui-help">{runsInHelp(agentName)}</p>
    {more && <p className="ui-help">Only the first 100 conversations where you may ask {agentName} are listed.</p>}
  </fieldset>;
}
