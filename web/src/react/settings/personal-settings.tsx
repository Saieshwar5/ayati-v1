import { useState } from 'react';
import { chatRequest } from '../chat/model';
import { useMutation, useResource } from '../requests';
import { setNotifyDefaults } from '../chat/notifications';
import { workspaceSession } from '../../workspace-context';
import { MyProfile } from './my-profile';

export function MyAccount() {
  const user = workspaceSession?.user;
  if (!user) return null;
  return <section className="settings-account"><h2>Profile</h2><p>How colleagues see you in this workspace.</p>
    <MyProfile />
    <div className="settings-readonly"><span>Email address</span><strong>{user.email}</strong><p>Used to sign in to your account.</p></div>
  </section>;
}

/** Preferences (designs/settings 2): theme on this device, notification
 * defaults on the server so they follow you. Each choice saves as you go. */
export function Preferences() {
  const [theme, setTheme] = useState(document.documentElement.dataset.theme ?? 'system');
  const [status, setStatus] = useState('');
  function change(value: string) {
    setTheme(value);
    if (value === 'system') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = value;
    try { localStorage.setItem('ayati.theme', value); setStatus('Appearance saved on this device.'); }
    catch { setStatus('Appearance applied. This browser could not save it for next time.'); }
  }
  return <section className="settings-preferences"><h2>Preferences</h2><p>Changes save as you go.</p>
    <fieldset className="settings-themes"><legend>Appearance</legend><p className="ui-help">Saved on this device.</p>{['light', 'dark', 'system'].map(value => <label className={`settings-theme theme-${value}`} key={value}>
    <span className="settings-theme-preview" aria-hidden="true"><i /><span><i /><i /></span></span>
    <span>{value[0].toUpperCase() + value.slice(1)}<input type="radio" name="theme" value={value} checked={theme === value} onChange={() => change(value)} /></span>
  </label>)}</fieldset><p role="status">{status}</p>
    <NotificationDefaults />
  </section>;
}

type Saved = { desktop_alerts: boolean; channel_level: string; approval_alerts: boolean };
const loadPreferences = () => chatRequest<Saved>('/preferences');

function NotificationDefaults() {
  const resource = useResource(loadPreferences);
  const mutation = useMutation();
  const data = resource.data;
  const save = (change: Partial<Saved>) => {
    if (!data) return;
    const next = { ...data, ...change };
    void mutation.run(async () => { resource.setData(await chatRequest<Saved>('/preferences', next)); setNotifyDefaults(next); mutation.setStatus('Saved.'); });
  };
  return <section className="settings-notifications" aria-label="Notifications"><h3>Notifications</h3>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!data && !resource.error && <p role="status">Loading notifications…</p>}
    {data && <fieldset className="ui-fieldset settings-rows" disabled={mutation.busy}>
      <div className="settings-row"><span><strong>Desktop alerts</strong><small>Show alerts while Cuckoo is open in a tab.</small></span>
        <label className="ui-switch"><input type="checkbox" role="switch" checked={data.desktop_alerts} onChange={event => save({ desktop_alerts: event.target.checked })} /><span>Desktop alerts</span></label></div>
      <div className="settings-row"><span><strong>Direct messages</strong><small>Always notify, from people and agents.</small></span><span className="ui-badge">Always</span></div>
      <div className="settings-row"><span><strong>Channels</strong><small>Default for channels; change any channel from its bell.</small></span>
        <select aria-label="Channels" value={data.channel_level} onChange={event => save({ channel_level: event.target.value })}>
          <option value="all">Every message</option><option value="mentions">Mentions and replies</option><option value="none">Nothing</option></select></div>
      <div className="settings-row"><span><strong>Approvals for agents I manage</strong><small>Requests to approve an agent’s action.</small></span>
        <label className="ui-switch"><input type="checkbox" role="switch" checked={data.approval_alerts} onChange={event => save({ approval_alerts: event.target.checked })} /><span>Approvals for agents I manage</span></label></div>
      <div className="settings-row"><span><strong>Email alerts</strong><small>When you are away from Cuckoo.</small></span><span className="ui-badge settings-soon">Coming later</span></div>
    </fieldset>}
    <p className="ui-status" role="status">{mutation.status}</p>
  </section>;
}
