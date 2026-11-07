import { useState } from 'react';
import { useMutation, useResource } from '../react/requests';
import { codesRequest, type Code } from './api';
import { Loading, Tag } from './shared';

const load = () => codesRequest<{ codes: Code[] }>('/admin/workspace-codes');
const date = (seconds: number) => new Date(seconds * 1000).toLocaleDateString([], { day: 'numeric', month: 'short', year: 'numeric' });
const TONE = { unused: 'ok', used: 'off', expired: 'off', revoked: 'bad' } as const;

/** The existing invite-code API, inside the console; both write the change log. */
export function CodesScreen() {
  const { data, error, refresh } = useResource(load);
  const [note, setNote] = useState('');
  const [days, setDays] = useState(14);
  const [issued, setIssued] = useState<{ code: string; note: string }>();
  const [copied, setCopied] = useState(false);
  const mutation = useMutation();
  return <div className="cc-stack cc-narrow-page">
    <form className="cc-card cc-pad cc-inline-form" onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
      const result = await codesRequest<{ code: string; note: string }>('/admin/workspace-codes', { note, days });
      setIssued(result); setCopied(false); setNote(''); await refresh();
    }); }}>
      <label className="cc-field cc-grow-2">For (company or person)<input className="cc-input" required maxLength={120} value={note} placeholder="Mehta Furnishings"
        onChange={event => setNote(event.target.value)} disabled={mutation.busy} /></label>
      <label className="cc-field">Expires after<select className="cc-input" value={days} onChange={event => setDays(Number(event.target.value))} disabled={mutation.busy}>
        {[7, 14, 30, 60, 90].map(n => <option key={n} value={n}>{n} days</option>)}</select></label>
      <button type="submit" className="cc-btn primary" disabled={mutation.busy || !note.trim()}>{mutation.busy ? 'Issuing…' : 'Issue code'}</button>
      {mutation.status && <p className="cc-alert cc-full" role="alert">{mutation.status}</p>}
      {issued && <div className="cc-issued cc-full" role="status">
        <span>Code for {issued.note}. Copy it now: it is not shown again.</span><code className="cc-mono">{issued.code}</code>
        <button type="button" className="cc-btn" onClick={() => void navigator.clipboard?.writeText(issued.code).then(() => setCopied(true))}>{copied ? 'Copied' : 'Copy code'}</button>
      </div>}
    </form>
    <section className="cc-card cc-scroll" aria-label="Invite codes">
      {!data ? <Loading error={error} label="codes" /> : !data.codes.length ? <p className="cc-empty">No codes yet. Issue one for the first company you onboard.</p> :
        <table className="cc-table"><thead><tr><th scope="col">For</th><th scope="col">State</th><th scope="col">Expires</th><th scope="col">Became</th><th scope="col"><span className="cc-visually-hidden">Actions</span></th></tr></thead>
          <tbody>{data.codes.map(code => <tr key={code.id}>
            <td><strong>{code.note}</strong><div className="cc-dim cc-small">issued {date(code.created_at)}</div></td>
            <td><Tag tone={TONE[code.status]}>{code.status.toUpperCase()}</Tag></td>
            <td className="cc-num cc-dim">{code.status === 'used' ? '—' : date(code.expires_at)}</td>
            <td>{code.workspace ? <>{code.workspace.name}<div className="cc-dim cc-small">{code.used_by?.email}</div></> : '—'}</td>
            <td>{code.status === 'unused' && <button type="button" className="cc-btn ghost" disabled={mutation.busy} aria-label={`Revoke code for ${code.note}`}
              onClick={() => void mutation.run(async () => { await codesRequest(`/admin/workspace-codes/${code.id}/revoke`, {}); await refresh(); })}>Revoke</button>}</td>
          </tr>)}</tbody></table>}
    </section>
  </div>;
}
