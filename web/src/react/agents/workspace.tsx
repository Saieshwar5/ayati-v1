import { useCallback } from 'react';
import { Button } from '../ui';
import { useResource } from '../requests';
import { Knowledge } from './knowledge';
import { loadControls } from './control-model';
import type { Agent } from './model';
import { AgentAvatar } from './avatar';
import { AgentWorkspaceHeader } from './workspace-header';
import { WorkspaceTabs, type WorkspaceSection } from './workspace-tabs';
import { AuthoritySummary } from './authority-summary';
import { AgentWork, useAgentRequests } from './work';
import { RequestRow } from './request-row';
import { AutomationLauncher } from './automation/launcher';

export function AgentWorkspace({ agent, revision, back, edit, guide, control, rehearse, openChat, busy, section, selectSection }: {
  agent: Agent; revision: number; back: () => void; edit: () => void; guide: () => void;
  control: () => void; rehearse: () => void; openChat: () => void; busy: boolean;
  section: WorkspaceSection; selectSection: (section: WorkspaceSection) => void;
}) {
  const load = useCallback(() => loadControls(agent.id), [agent.id, revision]);
  const controls = useResource(load);
  return <section className="agent-workspace" aria-label={`${agent.name} workspace`}>
    <AgentWorkspaceHeader name={agent.name} role={agent.role} avatar={<AgentAvatar agent={agent} />}
      status={agent.settings.paused ? 'Paused' : 'Available for requests'} owner={agent.owner_name} back={back}>
      <Button primary disabled={busy} onClick={openChat}>Open in Chat</Button>
      <Button aria-label={`View ${agent.name}`} onClick={edit}>{agent.editable ? 'Edit profile' : 'Profile'}</Button>
    </AgentWorkspaceHeader>
    <WorkspaceTabs selected={section} select={selectSection} />
    <div id="agent-workspace-panel" className="agent-workspace-panel" role="tabpanel" aria-label={section} tabIndex={0}>
      {section === 'Overview' && <AgentOverview agent={agent} viewWork={() => selectSection('Work')} controls={controls.data} controlsError={controls.error} guide={guide} control={control} rehearse={rehearse} />}
      {section === 'Work' && <AgentWork agent={agent.id} />}
      {section === 'Knowledge' && <Knowledge agent={agent} />}
      {section === 'Access' && <><h3>Access and limits</h3><p>Accounts, actions, authorized approvers and the people allowed to request work are separate controls.</p>
        <AuthoritySummary data={controls.data} error={controls.error} />
        {agent.editable && <Button aria-label={`Set up permissions for ${agent.name}`} onClick={guide}>Set up permissions</Button>}
        <Button aria-label={`Tools and approvals for ${agent.name}`} onClick={control}>Tools and approvals</Button></>}
      {section === 'Automation' && <AutomationLauncher agent={agent} />}
      {section === 'Environment' && <><h3>Working environment</h3><dl className="agent-facts"><dt>Model</dt><dd>{agent.model}</dd><dt>Knowledge</dt><dd>{agent.settings.memory ? 'Approved knowledge enabled' : 'Approved knowledge disabled'}</dd><dt>Browser and computer</dt><dd>Not connected for this agent</dd><dt>Business email and phone</dt><dd>Not connected</dd></dl><p className="ui-help">The agent keeps its identity and saved work between requests. A pending approval releases its execution worker.</p></>}
      {section === 'Activity' && <AgentWork agent={agent.id} activity />}
    </div>
  </section>;
}

function AgentOverview({ agent, viewWork, controls, controlsError, guide, control, rehearse }: {
  agent: Agent; viewWork: () => void; controls: import('./control-model').Controls | undefined;
  controlsError: string; guide: () => void; control: () => void; rehearse: () => void;
}) {
  const { data, error, refresh } = useAgentRequests(agent.id, 'mine', 'active');
  return <div className="agent-overview"><div className="agent-overview-main">
    <section><h3>Responsibility</h3><p className="agent-responsibility">{agent.role}</p>
      {agent.settings.successful_outcome && <><h4>Successful outcome</h4><p>{agent.settings.successful_outcome}</p></>}
      {agent.settings.escalation && <><h4>Ask a person when</h4><p>{agent.settings.escalation}</p></>}
      {agent.settings.routine && <><h4>Routine</h4><p>{agent.settings.routine}</p></>}
      {agent.editable && <Button onClick={rehearse}>Try a sample task</Button>}
    </section>
    <section><header className="agent-section-heading"><h3>My open requests</h3><Button onClick={viewWork}>View work</Button></header>
      {error && <p role="alert">{error}</p>}{!data && !error && <p role="status">Loading work…</p>}
      {data?.requests.slice(0, 3).map(request => <RequestRow key={request.id} request={request} refresh={refresh} compact />)}
      {data && !data.requests.length && <p className="ui-help">No open requests from you. Open Chat to give {agent.name} work.</p>}
      {data && data.requests.length > 3 && <Button onClick={viewWork}>View all my open requests</Button>}
    </section>
  </div><aside className="agent-overview-side">
    <section><h3>A place in your team</h3><dl className="agent-facts"><dt>Visibility</dt><dd>{agent.scope === 'private' ? 'Only you' : 'Workspace · people with access'}</dd><dt>Responsible person</dt><dd>{agent.owner_name ?? 'Not available'}</dd><dt>Business email</dt><dd>Not connected</dd><dt>Phone</dt><dd>Not connected</dd></dl></section>
    <AuthoritySummary data={controls} error={controlsError} />
    <div className="agent-profile-shortcuts">{agent.editable && <Button aria-label={`Set up permissions for ${agent.name}`} onClick={guide}>Set up permissions</Button>}
      <Button aria-label={`Tools and approvals for ${agent.name}`} onClick={control}>Tools and approvals</Button></div>
  </aside></div>;
}
