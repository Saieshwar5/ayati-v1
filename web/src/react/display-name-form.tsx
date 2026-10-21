import { useEffect, useRef, useState } from 'react';
import { identityRequest } from '../workspace-context';
import { useMutation } from './requests';

export type AccountUser = { id: string; email: string; display_name?: string | null };

export function DisplayNameForm({ user, saved }: { user: AccountUser; saved: (user: AccountUser) => void }) {
  const [name, setName] = useState(user.display_name ?? '');
  const mutation = useMutation();
  const alive = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  return <form className="account-profile-form" onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
    const updated = await identityRequest<AccountUser>('/profile', { display_name: name });
    if (!alive.current) return;
    setName(updated.display_name ?? '');
    mutation.setStatus('Display name saved.');
    saved(updated);
  }); }}>
    <fieldset className="ui-fieldset" disabled={mutation.busy}>
      <label className="access-field">Display name<input name="display_name" autoComplete="nickname" required maxLength={80} value={name} onChange={event => setName(event.target.value)} /></label>
      <p className="access-help">This is how you appear in messages and your team. You can change it anytime.</p>
      <button type="submit" className="access-button primary" disabled={!name.trim()}>{mutation.busy ? 'Saving…' : 'Save display name'}</button>
    </fieldset>
    <p className="access-help" role="status">{mutation.status}</p>
  </form>;
}
