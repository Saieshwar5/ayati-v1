import type { ReactNode } from 'react';
import { workspaceId, workspaceUrl, canManageResources } from '../workspace-context';
import { Brand } from './ui';
import { AccountMenu } from './account-menu';

export type WorkspacePage = 'home' | 'apps' | 'chat' | 'agents' | 'settings' | 'people' | 'account';
function Icon({ name }: { name: WorkspacePage }) {
  return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
    {name === 'home' && <><path d="m3 10 9-7 9 7v10H3Z" /><path d="M9 20v-7h6v7" /></>}
    {name === 'apps' && <>{[[3, 3], [14, 3], [3, 14], [14, 14]].map(([x, y]) => <rect key={`${x}-${y}`} x={x} y={y} width="7" height="7" rx="1" />)}</>}
    {name === 'chat' && <><path d="M21 11a8 8 0 0 1-8 8H5l-3 3V11a9 9 0 0 1 19 0Z" /><path d="M7 11h10" /></>}
    {name === 'agents' && <><rect x="4" y="7" width="16" height="14" rx="4" /><path d="M12 3v4M1 12v4m22-4v4M8 12v2m8-2v2m-8 3h8" /></>}
    {name === 'people' && <><circle cx="8" cy="7" r="3" /><path d="M2 21v-4a6 6 0 0 1 12 0v4m2-17a3 3 0 0 1 0 6m2 4a5 5 0 0 1 4 5v2" /></>}
    {name === 'settings' && <><path d="M3 6h18M3 12h18M3 18h18" /><circle cx="8" cy="6" r="2" /><circle cx="16" cy="12" r="2" /><circle cx="9" cy="18" r="2" /></>}
  </svg>;
}

export function WorkspaceShell({ page, children }: { page: WorkspacePage; children: ReactNode }) {
  const pages: WorkspacePage[] = workspaceId ? ['home', 'apps', 'chat', 'agents', 'people', 'settings'] : ['home', 'apps', 'chat', 'agents', 'settings'];
  return <div className="workspace-shell"><aside className="workspace-rail">
    <a className="workspace-brand" href={workspaceUrl('/')} aria-label="Relay home"><Brand symbol /></a>
    <nav aria-label="Workspace">{pages.map(name => {
      const label = name === 'people' ? 'Users' : name[0].toUpperCase() + name.slice(1);
      return <a key={name} className={`workspace-nav nav-${name}`} href={workspaceUrl(`/?view=${name}`)} title={label}
        aria-current={page === name ? 'page' : undefined}><Icon name={name} /><span>{label}</span></a>;
    })}</nav>
    {canManageResources() && <button type="button" className="workspace-help" aria-label="Ask Relay" title="Ask Relay" onClick={() => void import('../guide').then(module => module.openGuide())}>?</button>}
    {workspaceId ? <AccountMenu /> : <span className="workspace-owner">You</span>}
  </aside><div className="workspace-body">{children}</div></div>;
}
