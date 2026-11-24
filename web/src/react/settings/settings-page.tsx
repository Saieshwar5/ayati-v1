import { useState } from 'react';
import { currentWorkspace, workspaceSession, workspaceUrl } from '../../workspace-context';
import { navigate } from '../../navigation';
import { PeoplePage } from '../people';
import { AuditLog } from './audit-log';
import { SimulationReport } from '../simulation-report';
import { WorkspaceGeneral } from './workspace-settings';
import { AgentRules } from './agent-rules';
import { ProfileSettings, Preferences } from './personal-settings';
import { ThisWorkspace } from './this-workspace';
import { ConnectionsSettings } from './apps-settings';
import { adminWorkspace, labels, memberWorkspace, personal, requestedSection, sectionUrl, type Section } from './settings-sections';
import { SettingsIcon } from './settings-icons';
import './settings.css';
import { UsagePage } from './usage-page';
import { MemoryBrowser } from '../memory/memory-browser';

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
        <SettingsIcon section={item} /><span>{labels[item]}</span>
      </button></li>)}</ul></div>;
  // On phones the list is the first screen; a named page replaces it, with a way back.
  return <main className="settings-page settings-react" aria-label="Settings" data-showing={asked ? 'page' : 'list'}>
    <nav className="settings-sidebar" aria-label="Settings pages">
      <header className="settings-heading"><h1>Settings</h1><p>{workspaceName || 'Workspace'} · {admin ? 'Admin' : 'Member'}</p></header>
      {group('You', you)}{group('Workspace', workspace)}
    </nav>
    <div className="settings-detail" key={section}>
      <button type="button" className="settings-back" onClick={() => go()}>‹ Settings</button>
      {section === 'profile' ? <ProfileSettings />
        : section === 'preferences' ? <Preferences />
        : section === 'this-workspace' ? <ThisWorkspace />
        : section === 'workspace' ? <WorkspaceGeneral renamed={name => { const entry = currentWorkspace(); if (entry) entry.name = name; setWorkspaceName(name); }} />
        : section === 'people' ? <PeoplePage />
        : section === 'connections' ? <ConnectionsSettings />
        : section === 'company-rules' ? <AgentRules />
        : section === 'memory' ? <section className="settings-memory"><h2>Memory</h2>
          <p>What your agents have learned about this workspace. Every agent uses it; only admins see it here.</p>
          <MemoryBrowser /></section>
        : section === 'audit-log' ? <><AuditLog />{workspaceSession?.simulation && <SimulationReport />}</>
        : <UsagePage />}
    </div>
  </main>;
}
