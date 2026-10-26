import { useCallback, useEffect } from 'react';
import type { Profile } from '../../agents';
import { api, type Snapshot } from '../../types';
import { workspaceUrl } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { AgentWorkspaceHeader } from './workspace-header';
import { WorkspaceTabs, type WorkspaceSection } from './workspace-tabs';

export function LocalAgentWorkspace({ profile, back, edit, section, selectSection }: { profile: Profile; back: () => void; edit: () => void; section: WorkspaceSection; selectSection: (section: WorkspaceSection) => void }) {
  const chat = workspaceUrl(`/?view=chat&agent=${encodeURIComponent(profile.id)}`);
  return <section className="agent-workspace" aria-label={`${profile.name} local workspace`}>
    <AgentWorkspaceHeader name={profile.name} role={profile.role} avatar={<span className="ui-avatar agent-profile-avatar">✦</span>}
      status={profile.state} back={back}>
      <a className="ui-button primary" href={chat}>Open in Chat</a><Button onClick={edit}>Edit local profile</Button>
    </AgentWorkspaceHeader>
    <WorkspaceTabs selected={section} select={selectSection} />
    <div id="agent-workspace-panel" className="agent-workspace-panel" role="tabpanel" aria-label={section} tabIndex={0}>
      {section === 'Overview' && <div className="agent-overview"><div><h3>Responsibility</h3><p className="agent-responsibility">{profile.role}</p>
        <h3>Existing private agent</h3><p>Saved work, files, connections and the working environment stay with this agent.</p>
        <Button onClick={() => selectSection('Work')}>View work</Button></div><aside className="agent-overview-side"><h3>Working environment</h3><p>{profile.detail || profile.state}</p>
        <p>Knowledge: {profile.memory ? 'Enabled' : 'Disabled'}</p><p>Browser: {profile.browser ? 'Enabled' : 'Disabled'}</p></aside></div>}
      {['Work', 'Activity'].includes(section) && <LocalWork key={section} profile={profile} history={section === 'Activity'} />}
      {section === 'Knowledge' && <><h3>Knowledge and memory</h3><p>{profile.memory ? 'Scoped memory is enabled.' : 'Memory is disabled. Saved records are retained.'}</p><p>Inspect sources, teach, correct or forget through this agent’s existing private Chat tools.</p><a className="ui-button" href={chat}>Open private Chat tools</a></>}
      {section === 'Access' && <><h3>Access and limits</h3><p>This existing agent uses its own saved connections, approvals and private work environment.</p><p>Manage those controls through its private Chat tools. Workspace business-action permissions remain a separate system.</p><a className="ui-button" href={chat}>Open access controls in Chat</a></>}
      {section === 'Automation' && <><h3>Schedules and triggers</h3><p>Existing saved routines and connected triggers remain available in this agent’s Chat tools.</p><a className="ui-button" href={chat}>Open automation in Chat</a></>}
      {section === 'Environment' && <><h3>Private working environment</h3><dl className="agent-facts"><dt>Model</dt><dd>{profile.model}</dd><dt>Browser</dt><dd>{profile.browser ? 'Enabled' : 'Disabled'}</dd><dt>Current state</dt><dd>{profile.state}</dd></dl><p>{profile.detail}</p><a className="ui-button" href={chat}>Open files, browser and computer tools</a></>}
    </div>
  </section>;
}

function LocalWork({ profile, history }: { profile: Profile; history: boolean }) {
  const load = useCallback(() => api<Snapshot>('/state', undefined, profile.id), [profile.id]);
  const { data, error, refresh, setData } = useResource(load);
  const mutation = useMutation();
  useEffect(() => { if (error) setData(undefined); }, [error, setData]);
  useEffect(() => {
    const timer = window.setInterval(() => { if (!document.hidden) void refresh().catch(() => {}); }, 3000);
    return () => clearInterval(timer);
  }, [refresh]);
  const tasks = data?.tasks.filter(task => history || ['queued', 'running', 'awaiting_permission', 'waiting'].includes(task.status)) ?? [];
  return <section aria-label={history ? 'Local request history' : 'Local active requests'}><header className="agent-section-heading"><h3>{history ? 'Recent private requests' : 'Active private requests'}</h3><Button onClick={() => void refresh().catch(() => {})}>Refresh progress</Button></header>
    <p className="ui-help">These requests belong to this existing private agent. The latest 100 saved tasks are available here.</p>
    {error && <p role="alert">{error}</p>}{mutation.status && <p role="alert">{mutation.status}</p>}
    {!data && !error && <p role="status">Loading private requests…</p>}
    {data && !tasks.length && <p className="ui-empty">No requests in this view.</p>}
    {tasks.map(task => {
      const message = data?.messages.find(m => m.task_id === task.id && m.role === 'user');
      return <article className="agent-work-row" key={task.id}><div className="agent-work-heading"><strong>{message?.content.slice(0, 180) || 'Private agent request'}</strong><span className="agent-work-state">{task.status}</span></div>
        <p className="agent-work-meta">{new Date(task.created_at).toLocaleString()}</p><p className="agent-work-update">{task.detail}</p>
        <div className="agent-work-actions"><a className="ui-button" href={workspaceUrl(`/?view=chat&agent=${encodeURIComponent(profile.id)}`)}>Open conversation</a>
          {['queued', 'running', 'awaiting_permission', 'waiting'].includes(task.status) && <Button disabled={mutation.busy} onClick={() => void mutation.run(async () => {
            await api(`/tasks/${encodeURIComponent(task.id)}/stop`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' }, profile.id);
            await refresh();
          })}>Stop request</Button>}
        </div></article>;
    })}
  </section>;
}
