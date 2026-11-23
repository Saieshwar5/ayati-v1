import { useState } from 'react';
import { activeStatus, usePresence } from '../chat/presence';
import { clearsPhrase, statusEmoji, statusLabel } from '../chat/presence-view';
import { StatusDialog } from '../chat/status-dialog';
import { Button } from '../ui';
import type { Preferences } from './use-preferences';

/** Profile → Status and presence: your status (the same dialog as the account menu)
 * and whether colleagues see you online. Both save at once, apart from the form. */
export function StatusAndPresence({ userId, preferences }: { userId: string; preferences: Preferences }) {
  const [choosing, setChoosing] = useState(false);
  const status = activeStatus(usePresence(userId));
  const ends = status ? clearsPhrase(status) : '';
  const { data, busy, save } = preferences;
  return <section className="my-profile-group" aria-label="Status and presence"><h3>Status and presence</h3>
    <div className="settings-rows">
      <div className="settings-row"><span><strong>{status ? <><span aria-hidden="true">{statusEmoji(status)}</span> {statusLabel(status)}</> : 'No status'}</strong>
        <small>{status ? ends ? ends[0].toUpperCase() + ends.slice(1) : 'Until you clear it' : 'Tell colleagues when you are busy or away.'}</small></span>
        <Button onClick={() => setChoosing(true)}>{status ? 'Change status' : 'Set a status'}</Button></div>
      {data && <div className="settings-row"><span><strong>Show when I’m online</strong><small>Off: nobody sees a green dot or “last seen” for you. Your status still shows.</small></span>
        <label className="ui-switch"><input type="checkbox" role="switch" disabled={busy} checked={data.show_online !== false} onChange={event => save({ show_online: event.target.checked })} /><span>Show when I’m online</span></label></div>}
    </div>
    {preferences.error && <p role="alert">{preferences.error}</p>}
    <p className="ui-status" role="status">{preferences.status}</p>
    {choosing && <StatusDialog close={() => setChoosing(false)} />}
  </section>;
}
