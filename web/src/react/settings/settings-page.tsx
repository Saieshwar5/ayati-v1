import { useState } from 'react';
import { currentWorkspace, workspaceSession, workspaceUrl } from '../../workspace-context';
import { navigate } from '../../navigation';
import { PeoplePage } from '../people';
import { HelpPage } from './help-page';
import { AuditLog } from './audit-log';
import { SimulationReport } from '../simulation-report';
import { WorkspaceGeneral } from './workspace-settings';
import { WorkspaceConnections } from './workspace-connections';
import { MyAccount, Preferences } from './personal-settings';
import { ComingLater, ThisWorkspace } from './this-workspace';
import { adminWorkspace, comingLater, labels, memberWorkspace, personal, requestedSection, sectionUrl, type Section } from './settings-sections';
import './settings.css';

export function SettingsPage({ href }: { href: string }) {
  const url = new URL(href);
  const [workspaceName, setWorkspaceName] = useState(currentWorkspace()?.name ?? '');
  const admin = currentWorkspace()?.role === 'Admin';
  const you = personal.filter(section => section !== 'profile' || workspaceSession?.user);
  const workspace = admin ? adminWorkspace : memberWorkspace;
  const asked = requestedSection(url);
  // Admin pages fall back to the first page for members; the server refuses them as well.
  const section = asked && [...you, ...workspace].includes(asked) ? asked : you[0];
  const go = (next?: Section) => navigate(workspaceUrl(sectionUrl(next)));
  const group = (title: string, items: Section[]) => <div className="settings-group">
    <p className="settings-group-title" id={`settings-group-${title}`}>{title}</p>
    <ul aria-labelledby={`settings-group-${title}`}>{items.map(item => <li key={item}>
      <button type="button" aria-current={item === section ? 'page' : undefined} onClick={() => go(item)}>
        <span>{labels[item]}</span>{comingLater.has(item) && <small className="settings-soon">Coming later</small>}
      </button></li>)}</ul></div>;
  // On phones the list is the first screen; a named page replaces it, with a way back.
  return <main className="settings-page settings-react" aria-label="Settings" data-showing={asked ? 'page' : 'list'}>
    <nav className="settings-sidebar" aria-label="Settings pages">
      <header className="settings-heading"><h1>Settings</h1><p>{workspaceName || 'Workspace'} · {admin ? 'Admin' : 'Member'}</p></header>
      {group('You', you)}{group('Workspace', workspace)}
    </nav>
    <div className="settings-detail" key={section}>
      <button type="button" className="settings-back" onClick={() => go()}>‹ Settings</button>
      {section === 'profile' ? <MyAccount />
        : section === 'preferences' ? <Preferences />
        : section === 'help' ? <HelpPage />
        : section === 'this-workspace' ? <ThisWorkspace />
        : section === 'workspace' ? <WorkspaceGeneral renamed={name => { const entry = currentWorkspace(); if (entry) entry.name = name; setWorkspaceName(name); }} />
        : section === 'people' ? <PeoplePage />
        : section === 'connections' ? <WorkspaceConnections />
        : section === 'audit-log' ? <><AuditLog />{workspaceSession?.simulation && <SimulationReport />}</>
        : <ComingLater section={section} />}
    </div>
  </main>;
}
