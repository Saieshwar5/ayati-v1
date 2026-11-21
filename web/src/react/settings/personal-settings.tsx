import { useState } from 'react';
import { currentWorkspace, workspaceSession, workspaceUrl } from '../../workspace-context';
import { MyProfile } from './my-profile';
import { sectionUrl } from './settings-sections';
import { usePreferences } from './use-preferences';

/** The one place to edit your name, photo, job title and About (D245). */
export function ProfileSettings() {
  if (!workspaceSession?.user) return null;
  return <section className="settings-profile"><h2>Profile</h2>
    <p>How colleagues and agents see you in {currentWorkspace()?.name || 'this workspace'}.</p>
    <MyProfile />
  </section>;
}

/** Preferences: theme on this device, notification defaults on the server so they
 * follow you. Each choice saves as you go. Status and presence are on Profile. */
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
  return <section className="settings-preferences"><h2>Preferences</h2><p>Each change saves as you make it.</p>
    <fieldset className="settings-themes"><legend>Appearance <small>Saved on this device</small></legend>{['light', 'dark', 'system'].map(value => <label className={`settings-theme theme-${value}`} key={value}>
      <span className="settings-theme-preview" aria-hidden="true"><i /><span><i /><i /></span></span>
      <span>{value[0].toUpperCase() + value.slice(1)}<input type="radio" name="theme" value={value} checked={theme === value} onChange={() => change(value)} /></span>
    </label>)}</fieldset><p className="ui-status" role="status">{status}</p>
    <NotificationDefaults />
  </section>;
}

function NotificationDefaults() {
  const { data, error, busy, status, save } = usePreferences();
  return <section className="settings-notifications" aria-label="Notifications"><h3>Notifications</h3>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading notifications…</p>}
    {data && <fieldset className="ui-fieldset settings-rows" disabled={busy}>
      <div className="settings-row"><span><strong>Desktop alerts</strong><small>Show an alert while Cuckoo is open in a tab.</small></span>
        <label className="ui-switch"><input type="checkbox" role="switch" checked={data.desktop_alerts} onChange={event => save({ desktop_alerts: event.target.checked })} /><span>Desktop alerts</span></label></div>
      <div className="settings-row"><span><strong>Channels</strong><small>The default. Change one channel from its bell.</small></span>
        <select aria-label="Channels" value={data.channel_level} onChange={event => save({ channel_level: event.target.value })}>
          <option value="all">Every message</option><option value="mentions">Mentions and replies</option><option value="none">Nothing</option></select></div>
      <div className="settings-row"><span><strong>Approvals for agents I manage</strong><small>When an agent asks before it acts.</small></span>
        <label className="ui-switch"><input type="checkbox" role="switch" checked={data.approval_alerts} onChange={event => save({ approval_alerts: event.target.checked })} /><span>Approvals for agents I manage</span></label></div>
    </fieldset>}
    <p className="ui-help">Direct messages always notify you. Your status and “Show when I’m online” are on <a href={workspaceUrl(sectionUrl('profile'))}>Profile</a>.</p>
    <p className="ui-status" role="status">{status}</p>
  </section>;
}
