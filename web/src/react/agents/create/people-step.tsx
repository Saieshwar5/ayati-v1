import { useState } from 'react';
import { Button } from '../../ui';
import { identityRequest, workspaceId } from '../../../workspace-context';
import type { Audience } from '../model';

export type Member = { id: string; name: string; email?: string };
/** Everyone in the workspace, for choosing selected people and managers. */
export const loadMembers = () => identityRequest<{ users: Member[] }>(`/workspaces/${workspaceId}/chat-directory`);

const choices: { value: Audience; label: (workspace: string) => string; help: (name: string) => string }[] = [
  { value: 'everyone', label: workspace => `Everyone in ${workspace}`, help: name => `Any member can message ${name} or add it to a channel.` },
  { value: 'selected', label: () => 'Selected people', help: name => `Choose who can message ${name}.` },
  { value: 'only_me', label: () => 'Only me', help: name => `Only you can message ${name}. Admins can still see its settings and activity.` },
];

/** Create step 3: who can talk to it, its managers and who sees its activity
 * (design 3-create-people). The creator owns the agent and starts as a manager. */
export function PeopleStep({ name, workspace, me, owner = me, members, audience, people, managers, setAudience, setPeople, setManagers }: {
  name: string; workspace: string; me: string; owner?: string; members?: Member[];
  audience: Audience; people: string[]; managers: string[];
  setAudience: (value: Audience) => void; setPeople: (ids: string[]) => void; setManagers: (ids: string[]) => void;
}) {
  const [adding, setAdding] = useState('');
  const display = name.trim() || 'this agent';
  const others = (members ?? []).filter(member => member.id !== owner);
  const named = (id: string) => members?.find(member => member.id === id);
  const candidates = others.filter(member => !managers.includes(member.id));
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
      <section className="agent-managers" aria-label="Managers">
        <h3>Managers</h3>
        <p className="ui-help">Managers approve {display}’s “Ask first” actions, see all its activity and can pause it. If no manager decides within 4 hours, an admin can.</p>
        <ul>{managers.map(id => <li key={id}>
          <span><strong>{named(id)?.name ?? (id === me ? 'You' : 'Former member')}{id === me && ' (you)'}</strong>{named(id)?.email && <small>{named(id)?.email}</small>}</span>
          {id === owner ? <span className="agent-owner-tag">Owner</span>
            : <Button className="ui-icon" aria-label={`Remove ${named(id)?.name ?? 'manager'} as a manager`} disabled={managers.length < 2}
              onClick={() => setManagers(managers.filter(other => other !== id))}>×</Button>}
        </li>)}</ul>
        {candidates.length > 0 && <div className="agent-add-manager">
          <label className="ui-field">Add manager<select value={adding} onChange={event => setAdding(event.target.value)}>
            <option value="">Choose a person</option>
            {candidates.map(member => <option key={member.id} value={member.id}>{member.name}</option>)}
          </select></label>
          <Button disabled={!adding} onClick={() => { setManagers([...managers, adding]); setAdding(''); }}>Add</Button>
        </div>}
      </section>
    </div>
    <aside className="agent-create-side">
      <section className="agent-rules" aria-label={`Who sees ${display}’s activity`}>
        <h3>Who sees {display}’s activity</h3>
        <dl><dt>You and other managers</dt><dd>Everything {display} does.</dd>
          <dt>Admins</dt><dd>Everything {display} does.</dd>
          <dt>Everyone else</dt><dd>Only their own requests.</dd></dl>
      </section>
    </aside>
  </div>;
}
