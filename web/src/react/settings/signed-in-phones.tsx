import { useCallback } from 'react';
import { identityRequest } from '../../workspace-context';
import { useMutation, useResource } from '../requests';
import { Button } from '../ui';

type Phone = { id: string; name: string; platform: 'ios' | 'android'; last_used: number; notifications?: boolean };

/** Phones signed in to this account with the Cuckoo app (runtime/mobile-sign-in.md).
 * Signing one out here ends its session and its notifications; it can sign in again. */
export function SignedInPhones() {
  const load = useCallback(() => identityRequest<{ devices: Phone[] }>('/devices'), []);
  const { data, error, refresh } = useResource(load);
  const mutation = useMutation();
  const signOut = (phone: Phone) => mutation.run(async () => {
    await identityRequest(`/devices/${encodeURIComponent(phone.id)}/revoke`, {});
    await refresh();
    mutation.setStatus(`${phone.name} is signed out.`);
  });
  return <section className="settings-phones" aria-label="Signed-in phones"><h3>Signed-in phones</h3>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading phones…</p>}
    {data && !data.devices.length && <p className="ui-help">No phones are signed in with the Cuckoo app.</p>}
    {data && data.devices.length > 0 && <div className="settings-rows">
      {data.devices.map(phone => <div className="settings-row" key={phone.id}>
        <span><strong>{phone.name}</strong>
          <small>{phone.platform === 'ios' ? 'iPhone or iPad' : 'Android'} · last used {new Date(phone.last_used * 1000).toLocaleString([], { dateStyle: 'medium', timeStyle: 'short' })}{phone.notifications ? ' · notifications on' : ''}</small></span>
        <Button disabled={mutation.busy} onClick={() => void signOut(phone)}>Sign out</Button>
      </div>)}
    </div>}
    <p className="ui-status" role="status">{mutation.status}</p>
  </section>;
}
