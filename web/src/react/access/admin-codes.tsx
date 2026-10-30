import { useState } from 'react';
import { useMutation, useResource } from '../requests';
import { accessRequest, type WorkspaceCode } from './model';

const loadCodes = () => accessRequest<{ codes: WorkspaceCode[] }>('/admin/workspace-codes');
const date = (seconds: number) => new Date(seconds * 1000).toLocaleDateString([], { day: 'numeric', month: 'short', year: 'numeric' });
const STATUS: Record<WorkspaceCode['status'], string> = { unused: 'Unused', used: 'Used', expired: 'Expired', revoked: 'Revoked' };

/** Platform administrators issue, see and revoke workspace invite codes. */
export function AdminCodes({ back }: { back: () => void }) {
  const { data, error, refresh } = useResource(loadCodes);
  const [note, setNote] = useState('');
  const [validDays, setValidDays] = useState(14);
  const [issued, setIssued] = useState<{ code: string; note: string }>();
  const [copied, setCopied] = useState(false);
  const mutation = useMutation();
  return <div className="access-column wide">
    <div className="access-heading">
      <div><h1>Cuckoo admin</h1><p className="access-lead">Workspace invite codes. Only platform administrators see this page.</p></div>
      <button type="button" className="access-button quiet" onClick={back}>Back to workspaces</button>
    </div>
    <form className="access-box access-issue" onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
      const result = await accessRequest<{ code: string; note: string }>('/admin/workspace-codes', { note, days: validDays });
      setIssued(result); setCopied(false); setNote(''); await refresh();
    }); }}>
      <h2>New invite code</h2>
      <fieldset className="ui-fieldset access-issue-fields" disabled={mutation.busy}>
        <label className="access-field">For (company or person)
          <input required maxLength={120} placeholder="Mehta Furnishings" value={note} onChange={event => setNote(event.target.value)} />
        </label>
        <label className="access-field">Expires after
          <select value={validDays} onChange={event => setValidDays(Number(event.target.value))}>
            {[7, 14, 30, 60, 90].map(n => <option key={n} value={n}>{n} days</option>)}
          </select>
        </label>
        <button type="submit" className="access-button primary">Create code</button>
      </fieldset>
      {issued && <div className="access-issued" role="status">
        <span>Code for {issued.note}. Copy it now: it is not shown again.</span>
        <code>{issued.code}</code>
        <button type="button" className="access-button" onClick={() => void navigator.clipboard?.writeText(issued.code).then(() => setCopied(true))}>{copied ? 'Copied' : 'Copy code'}</button>
      </div>}
      {mutation.status && <p className="access-alert" role="alert">{mutation.status}</p>}
    </form>
    <section aria-labelledby="access-codes">
      <h2 id="access-codes" className="access-section">Codes</h2>
      {error && <p className="access-alert" role="alert">{error}</p>}
      {!data && !error && <p role="status">Loading codes…</p>}
      {data && !data.codes.length && <p className="access-help">No codes yet. Create one for the first company you onboard.</p>}
      {data && data.codes.length > 0 && <div className="access-table" role="table" aria-label="Workspace invite codes">
        <div role="row" className="access-table-head"><span role="columnheader">For</span><span role="columnheader">Status</span><span role="columnheader">Expires</span><span role="columnheader">Workspace</span><span role="columnheader"><span className="visually-hidden">Actions</span></span></div>
        {data.codes.map(item => <div role="row" key={item.id}>
          <span role="cell"><strong>{item.note}</strong><small>Created {date(item.created_at)}</small></span>
          <span role="cell"><span className={`access-status ${item.status}`}>{STATUS[item.status]}</span></span>
          <span role="cell">{item.status === 'used' ? '—' : date(item.expires_at)}</span>
          <span role="cell">{item.workspace ? <><strong>{item.workspace.name}</strong><small>{item.used_by?.name}</small></> : '—'}</span>
          <span role="cell">{item.status === 'unused' && <button type="button" className="access-button quiet" disabled={mutation.busy}
            aria-label={`Revoke code for ${item.note}`} onClick={() => void mutation.run(async () => {
              await accessRequest(`/admin/workspace-codes/${item.id}/revoke`, {}); await refresh();
            })}>Revoke</button>}</span>
        </div>)}
      </div>}
    </section>
  </div>;
}
