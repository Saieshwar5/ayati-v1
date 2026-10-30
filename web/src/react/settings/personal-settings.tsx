import { useState } from 'react';
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
  return <section><h2>Preferences</h2><p>Choose how Cuckoo looks on this device.</p><fieldset className="settings-themes"><legend>Appearance</legend>{['light', 'dark', 'system'].map(value => <label className={`settings-theme theme-${value}`} key={value}>
    <span className="settings-theme-preview" aria-hidden="true"><i /><span><i /><i /></span></span>
    <span>{value[0].toUpperCase() + value.slice(1)}<input type="radio" name="theme" value={value} checked={theme === value} onChange={() => change(value)} /></span>
  </label>)}</fieldset><p className="ui-help">Changes apply only to this device.</p><p role="status">{status}</p></section>;
}
