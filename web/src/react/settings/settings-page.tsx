import { useState } from 'react';
import { currentWorkspace, workspaceSession, workspaceUrl } from '../../workspace-context';
import { navigate } from '../../navigation';
import { MyProfile } from './my-profile';
import { PeoplePage } from '../people';
import { FeedbackPage } from '../feedback';
import { WorkspaceGeneral, WorkspaceActivity } from './workspace-settings';
import { WorkspaceConnections } from './workspace-connections';
import { WorkspaceTeams } from './workspace-teams';
import './settings.css';

type Scope = 'personal' | 'workspace';
const personal = ['My account', 'Appearance', 'Help & feedback'];
const workspace = ['General', 'People', 'Access', 'Connections', 'Activity', 'Teams'];
export function SettingsPage({ href }: { href: string }) {
  const url = new URL(href);
  const [workspaceName, setWorkspaceName] = useState(currentWorkspace()?.name ?? '');
  const owner = currentWorkspace()?.role === 'Owner';
  const legacy = url.searchParams.get('view');
  const requestedScope = url.searchParams.get('scope') ?? (legacy === 'people' || legacy === 'users' || legacy === 'company' ? 'workspace' : 'personal');
  const scope: Scope = requestedScope === 'workspace' && owner ? 'workspace' : 'personal';
  const requestedSection = legacy === 'people' || legacy === 'users' ? 'people' : legacy === 'company' ? 'teams' : legacy === 'feedback' ? 'help-&-feedback' : url.searchParams.get('section') ?? (url.hash === '#theme' ? 'appearance' : '');
  const sections = scope === 'personal' ? personal.filter(label => label !== 'My account' || workspaceSession?.user) : workspace;
  const section = sections.find(label => slug(label) === requestedSection) ?? sections[0];
  const select = (nextScope: Scope, nextSection?: string) => navigate(workspaceUrl(`/?view=settings&scope=${nextScope}${nextSection ? `&section=${slug(nextSection)}` : ''}`));
  return <main className="settings-page settings-react" aria-label="Settings">
    <header className="settings-heading"><h1>Settings</h1><p>Your preferences{owner ? ' and workspace, in one place.' : ', just the way you like them.'}</p></header>
    <div className="settings-controls">
      <div className="settings-scope" role="tablist" aria-label="Settings scope">
        {(['personal', ...(owner ? ['workspace'] : [])] as Scope[]).map(item => <button key={item} type="button" role="tab" aria-selected={scope === item} tabIndex={scope === item ? 0 : -1} aria-controls="settings-sections" id={`settings-${item}`} onClick={() => select(item)} onKeyDown={event => {
          if (owner && ['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) { event.preventDefault(); const next = event.key === 'Home' ? 'personal' : event.key === 'End' ? 'workspace' : item === 'personal' ? 'workspace' : 'personal'; select(next); document.getElementById(`settings-${next}`)?.focus(); }
        }}>{item === 'personal' ? 'Personal' : 'Workspace'}</button>)}
      </div>
      <span className="settings-context">{scope === 'personal' ? 'Your preferences' : workspaceName}</span>
    </div>
    <nav id="settings-sections" className="settings-sections" aria-label={`${scope === 'personal' ? 'Personal' : 'Workspace'} settings`}>
      {sections.map(label => <button key={label} type="button" aria-current={label === section ? 'page' : undefined} onClick={() => select(scope, label)}>{label}</button>)}
    </nav>
    <div className="settings-detail" key={`${scope}:${section}`}>
      {scope === 'personal' ? section === 'My account' ? <MyAccount /> : section === 'Appearance' ? <Appearance /> : <FeedbackPage />
        : section === 'People' || section === 'Access' ? <PeoplePage accessOnly={section === 'Access'} />
        : section === 'General' ? <WorkspaceGeneral renamed={name => { const entry = currentWorkspace(); if (entry) entry.name = name; setWorkspaceName(name); }} /> : section === 'Activity' ? <WorkspaceActivity />
        : section === 'Teams' ? <WorkspaceTeams /> : <WorkspaceConnections />}
    </div>
  </main>;
}
function slug(label: string) { return label.toLowerCase().replaceAll(' ', '-'); }
function MyAccount() {
  const user = workspaceSession?.user;
  if (!user) return null;
  return <section className="settings-account"><h2>My account</h2><p>How colleagues see you in this workspace.</p>
    <MyProfile />
    <div className="settings-readonly"><span>Email address</span><strong>{user.email}</strong><p>Used to sign in to your account.</p></div>
  </section>;
}
function Appearance() {
  const [theme, setTheme] = useState(document.documentElement.dataset.theme ?? 'system');
  const [status, setStatus] = useState('');
  function change(value: string) {
    setTheme(value);
    if (value === 'system') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = value;
    try { localStorage.setItem('ayati.theme', value); setStatus('Appearance saved on this device.'); }
    catch { setStatus('Appearance applied. This browser could not save it for next time.'); }
  }
  return <section><h2>Appearance</h2><p>Choose how Cuckoo looks on this device.</p><fieldset className="settings-themes"><legend>Theme</legend>{['light', 'dark', 'system'].map(value => <label className={`settings-theme theme-${value}`} key={value}>
    <span className="settings-theme-preview" aria-hidden="true"><i /><span><i /><i /></span></span>
    <span>{value[0].toUpperCase() + value.slice(1)}<input type="radio" name="theme" value={value} checked={theme === value} onChange={() => change(value)} /></span>
  </label>)}</fieldset><p className="ui-help">Changes apply only to this device.</p><p role="status">{status}</p></section>;
}
