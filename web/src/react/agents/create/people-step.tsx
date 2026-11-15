import { identityRequest, workspaceId } from '../../../workspace-context';
import type { Audience, Owner } from '../model';

export type Member = { id: string; name: string; email?: string };
/** Everyone in the workspace, for choosing selected people and owners. */
export const loadMembers = () => identityRequest<{ users: Member[] }>(`/workspaces/${workspaceId}/chat-directory`);

const choices: { value: Audience; label: (workspace: string) => string; help: (name: string) => string }[] = [
  { value: 'everyone', label: workspace => `Everyone in ${workspace}`, help: name => `Any member can message ${name} or add it to a channel.` },
  { value: 'selected', label: () => 'Selected people', help: name => `Choose who can message ${name}.` },
  { value: 'only_me', label: () => 'Only me', help: name => `Only ${name}’s owners can message it. Admins can still see its settings and activity.` },
];

/** Create step 3: who can talk to it, its owners and who sees its activity
 * (design 3-create-people). Its creator is its first owner; owners are equal and
 * more are added from the agent's page (schema 122). `owners` is unset while
 * creating. */
export function PeopleStep({ name, workspace, me, owners, members, audience, people, setAudience, setPeople }: {
  name: string; workspace: string; me: string; owners?: Owner[]; members?: Member[];
  audience: Audience; people: string[];
  setAudience: (value: Audience) => void; setPeople: (ids: string[]) => void;
}) {
  const display = name.trim() || 'this agent';
  const creating = owners === undefined;
  const listed = owners ?? [{ id: me, name: members?.find(member => member.id === me)?.name ?? 'You' }];
  // Owners can always talk to it, so only other people are chosen.
  const others = (members ?? []).filter(member => !listed.some(owner => owner.id === member.id));
  const email = (id: string) => members?.find(member => member.id === id)?.email;
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
      <section className="agent-managers" aria-label="Owners">
        <h3>Owners</h3>
        {listed.length ? <ul>{listed.map(owner => <li key={owner.id}>
          <span><strong>{owner.name}{owner.id === me && ' (you)'}</strong>{email(owner.id) && <small>{email(owner.id)}</small>}</span>
          <span className="agent-owner-tag">Owner</span></li>)}</ul>
          : <p className="ui-help">None: its last owner left the workspace.</p>}
        <p className="ui-help">Owners are equal: each one changes {display}, approves its “Ask first” actions, sees all its
          activity and can pause it, without asking the others. Admins can do the same, and decide an approval the owners
          haven’t answered in time. {creating ? `You can add owners from ${display}’s page once it is created.` : `Add or remove owners from ${display}’s Overview.`}
          {' '}If its last owner leaves the workspace, {display} pauses until an admin gives it a new owner.</p>
      </section>
    </div>
    <aside className="agent-create-side">
      <section className="agent-rules" aria-label={`Who sees ${display}’s activity`}>
        <h3>Who sees {display}’s activity</h3>
        <dl><dt>{creating ? 'You and other owners' : 'Its owners'}</dt><dd>Everything {display} does.</dd>
          <dt>Admins</dt><dd>Everything {display} does.</dd>
          <dt>Everyone else</dt><dd>Only their own requests.</dd></dl>
      </section>
    </aside>
  </div>;
}
