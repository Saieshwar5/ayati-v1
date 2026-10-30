import { useState } from 'react';
import { accessRequest } from './model';
import { useMutation } from '../requests';

/** Checks an invite code (without using it up) before the workspace is named. */
export function EnterCode({ email, empty, accepted, back, refresh }: {
  email: string; empty: boolean; accepted: (code: string, expires: number) => void;
  back?: () => void; refresh: () => void;
}) {
  const [code, setCode] = useState('');
  const mutation = useMutation();
  return <div className="access-column">
    <h1>{empty ? 'You’re signed in, but not in a workspace yet' : 'Create a workspace'}</h1>
    <p className="access-lead">Cuckoo is invite-only while we onboard our first companies.</p>
    <div className="access-cards">
      <form className="access-box" onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
        const result = await accessRequest<{ expires_at: number }>('/workspace-codes/check', { code });
        accepted(code, result.expires_at);
      }); }}>
        <h2>Have an invite code?</h2>
        <p className="access-help">The Cuckoo team gives each new company a code. One code creates one workspace.</p>
        <fieldset className="ui-fieldset" disabled={mutation.busy}>
          <label className="access-field">Invite code
            <input name="code" autoComplete="off" autoCapitalize="characters" spellCheck={false} placeholder="CKO-XXXX-XXXX-XXXX"
              required maxLength={40} value={code} onChange={event => setCode(event.target.value)} />
          </label>
          <button type="submit" className="access-button primary">{mutation.busy ? 'Checking…' : 'Continue'}</button>
        </fieldset>
        {mutation.status && <p className="access-alert" role="alert">{mutation.status}</p>}
      </form>
      <section className="access-box" aria-labelledby="access-expecting">
        <h2 id="access-expecting">Joining your company?</h2>
        <p className="access-help">Ask your company’s owner to invite <strong>{email}</strong>. The invitation appears here after you check again.</p>
        <button type="button" className="access-button" onClick={refresh}>Check again</button>
        {back && <button type="button" className="access-button quiet" onClick={back}>Back to your workspaces</button>}
      </section>
    </div>
  </div>;
}
