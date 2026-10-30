import type { Invitation, Workspace } from './model';
import { days, initials, tileColor } from './model';

function Tile({ id, name }: { id: string; name: string }) {
  return <span className="access-tile" style={{ background: tileColor(id) }} aria-hidden="true">{initials(name)}</span>;
}

const count = (n: number | undefined, one: string, many: string) => n === undefined ? '' : `${n} ${n === 1 ? one : many}`;

/** Invitations first, then the person's workspaces; opening one is the main action. */
export function WorkspaceList({ name, workspaces, invitations, last, busy, accept, decline, startCode, admin }: {
  name: string; workspaces: Workspace[]; invitations: Invitation[]; last?: string; busy: boolean;
  accept: (invitation: Invitation) => void; decline: (invitation: Invitation) => void;
  startCode: () => void; admin: boolean;
}) {
  return <div className="access-column">
    <h1>Welcome back, {name.split(' ')[0]}</h1>
    <p className="access-lead">{workspaces.length ? 'Choose a workspace to continue.' : 'Accept an invitation to get started.'}</p>
    {invitations.length > 0 && <section aria-labelledby="access-invitations">
      <h2 id="access-invitations" className="access-section">Invitations</h2>
      {invitations.map(invitation => <article key={invitation.id} className="access-row access-invitation">
        <Tile id={invitation.id} name={invitation.name} />
        <div className="access-row-text">
          <strong>{invitation.inviter ? `${invitation.inviter} invited you to ${invitation.name}` : `Invitation to ${invitation.name}`}</strong>
          <span>Join as {invitation.role === 'Owner' ? 'an owner' : 'a member'}{invitation.expires ? ` · expires in ${days(invitation.expires)} ${days(invitation.expires) === 1 ? 'day' : 'days'}` : ''}</span>
        </div>
        <div className="access-row-actions">
          <button type="button" className="access-button quiet" disabled={busy} onClick={() => decline(invitation)}>Decline</button>
          <button type="button" className="access-button primary" disabled={busy} onClick={() => accept(invitation)}>Accept</button>
        </div>
      </article>)}
    </section>}
    {workspaces.length > 0 && <section aria-labelledby="access-workspaces">
      <h2 id="access-workspaces" className="access-section">Your workspaces</h2>
      <nav aria-label="Your workspaces">{workspaces.map(workspace => <a key={workspace.id} className="access-row access-workspace" href={`/w/${workspace.id}/`}>
        <Tile id={workspace.id} name={workspace.name} />
        <span className="access-row-text">
          <strong>{workspace.name}{workspace.id === last && <small className="access-badge">Last opened</small>}</strong>
          <span>{[workspace.role, count(workspace.people, 'person', 'people'), count(workspace.agents, 'agent', 'agents')].filter(Boolean).join(' · ')}</span>
        </span>
        <span className="access-chevron" aria-hidden="true">›</span>
      </a>)}</nav>
    </section>}
    <p className="access-more">
      {admin ? <><button type="button" className="access-link" onClick={startCode}>Create a workspace</button> · <a className="access-link" href="/?view=account&admin=1">Cuckoo admin</a></>
        : <>Starting Cuckoo for your company? <button type="button" className="access-link" onClick={startCode}>Enter an invite code</button></>}
    </p>
  </div>;
}
