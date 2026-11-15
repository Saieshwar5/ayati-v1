import { identityRequest, workspaceId } from '../../../workspace-context';
import type { Audience } from '../model';

export type Member = { id: string; name: string; email?: string };
/** Everyone in the workspace, for choosing selected people and a new owner. */
export const loadMembers = () => identityRequest<{ users: Member[] }>(`/workspaces/${workspaceId}/chat-directory`);

const choices: { value: Audience; label: (workspace: string) => string; help: (name: string) => string }[] = [
  { value: 'everyone', label: workspace => `Everyone in ${workspace}`, help: name => `Any member can message ${name} or add it to a channel.` },
  { value: 'selected', label: () => 'Selected people', help: name => `Choose who can message ${name}.` },
  { value: 'only_me', label: () => 'Only me', help: name => `Only you can message ${name}. Admins can still see its settings and activity.` },
];

/** Create step 3: who can talk to it, its owner and who sees its activity
 * (design 3-create-people). An agent has one owner (schema 122): its creator,
 * until it is handed to someone else. */
export function PeopleStep({ name, workspace, me, owner = me, members, audience, people, setAudience, setPeople }: {
  name: string; workspace: string; me: string; owner?: string | null; members?: Member[];
  audience: Audience; people: string[];
  setAudience: (value: Audience) => void; setPeople: (ids: string[]) => void;
}) {
  const display = name.trim() || 'this agent';
  const others = (members ?? []).filter(member => member.id !== owner);
  const mine = owner === me;
  const person = members?.find(member => member.id === owner);
  const ownerName = person?.name ?? (mine ? 'You' : owner ? 'Former member' : 'Nobody yet');
  return <div className="agent-create-people">
    <div className="agent-create-main">
      <fieldset className="agent-audience"><legend>Who can talk to {display}</legend>
        {choices.map(choice => <label key={choice.value} className="agent-audience-choice">
          <input type="radio" name="audience" value={choice.value} checked={audience === choice.value} onChange={() => setAudience(choice.value)} />
          <span><strong>{choice.label(workspace)}</strong><small>{choice.help(display)}</small></span>
        </label>)}
      </fieldset>
      {audience === 'selected' && <fieldset className="agent-selected-people"><legend>People who can talk to {display}</legend>
        {!members && <p role="status">Loading people…</p>}
        {members && !others.length && <p className="ui-help">Nobody else is in this workspace yet.</p>}
        {others.map(member => <label key={member.id} className="ui-check">
          <input type="checkbox" checked={people.includes(member.id)}
            onChange={event => setPeople(event.target.checked ? [...people, member.id] : people.filter(id => id !== member.id))} />
          {member.name}</label>)}
      </fieldset>}
      {audience !== 'only_me' && <p className="ui-help">In a channel, anyone in that channel can ask {display}.</p>}
      <section className="agent-managers" aria-label="Owner">
        <h3>Owner</h3>
        <ul><li><span><strong>{ownerName}{mine && person && ' (you)'}</strong>{person?.email && <small>{person.email}</small>}</span>
          <span className="agent-owner-tag">Owner</span></li></ul>
        <p className="ui-help">{mine ? 'You change' : 'The owner changes'} {display}, approve{mine ? '' : 's'} its “Ask first” actions, see{mine ? '' : 's'} all its
          activity and can pause it. Admins can do the same, and decide an approval the owner hasn’t answered in time.
          If the owner leaves the workspace, {display} pauses until an admin gives it a new owner.</p>
      </section>
    </div>
    <aside className="agent-create-side">
      <section className="agent-rules" aria-label={`Who sees ${display}’s activity`}>
        <h3>Who sees {display}’s activity</h3>
        <dl><dt>{mine ? 'You, its owner' : 'Its owner'}</dt><dd>Everything {display} does.</dd>
          <dt>Admins</dt><dd>Everything {display} does.</dd>
          <dt>Everyone else</dt><dd>Only their own requests.</dd></dl>
      </section>
    </aside>
  </div>;
}
