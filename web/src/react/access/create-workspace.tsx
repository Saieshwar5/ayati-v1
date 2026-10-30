import { useRef, useState } from 'react';
import { accessRequest, days, initials, tileColor } from './model';
import { useMutation } from '../requests';
import { Notice } from './shell';

/** Names the workspace; the code is used up only when it is created. */
export function CreateWorkspace({ code, expires, back }: { code?: string; expires?: number; back: () => void }) {
  const [name, setName] = useState('');
  const request = useRef(crypto.randomUUID());
  const mutation = useMutation();
  const preview = name.trim() || 'Your company';
  return <div className="access-create">
    <form className="access-column" onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
      // The same request ID makes a retry return the workspace instead of failing.
      const result = await accessRequest<{ id: string }>('/workspaces', { name, request_id: request.current, ...(code ? { code } : {}) });
      location.assign(`/w/${result.id}/`);
    }); }}>
      <h1>Set up your workspace</h1>
      <p className="access-lead">{code && expires ? `Invite code accepted · valid for ${days(expires)} more ${days(expires) === 1 ? 'day' : 'days'}.` : 'You can create workspaces as a Cuckoo administrator.'}</p>
      <fieldset className="ui-fieldset" disabled={mutation.busy}>
        <label className="access-field">Workspace name
          <input name="name" required maxLength={80} autoComplete="organization" placeholder="Your company name" value={name} onChange={event => setName(event.target.value)} />
        </label>
        <Notice title="You will be the owner">After creating it you can invite as many people as you need, and add agents.</Notice>
        <div className="access-actions">
          <button type="submit" className="access-button primary">{mutation.busy ? 'Creating…' : 'Create workspace'}</button>
          <button type="button" className="access-button quiet" onClick={back}>Back</button>
        </div>
      </fieldset>
      {mutation.status && <p className="access-alert" role="alert">{mutation.status}</p>}
    </form>
    <aside className="access-preview-card" aria-label="Preview">
      <small>Preview</small>
      <div className="access-preview-head">
        <span className="access-tile large" style={{ background: tileColor(request.current) }} aria-hidden="true">{initials(preview)}</span>
        <strong>{preview}</strong>
      </div>
      <ul>
        <li>You are the owner; invite your team next</li>
        <li>Add agents that work beside your people</li>
        <li>Chat and Home are ready on day one</li>
      </ul>
    </aside>
  </div>;
}
