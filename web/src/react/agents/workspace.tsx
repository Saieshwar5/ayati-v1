import { Button } from '../ui';
import { useState } from 'react';
import { workspaceUrl } from '../../workspace-context';
import { useMutation } from '../requests';
import { saveAgent, type Agent, type Directory } from './model';
import { AgentWorkspaceHeader, type AgentAction } from './workspace-header';
import { WorkspaceTabs, type WorkspaceSection } from './workspace-tabs';
import { ConnectionsTab } from './connections-tab';
import { HistoryView } from '../activity/history';
import '../activity/activity.css';
import { AutomationLauncher } from './automation/launcher';
import { AgentOverview } from './overview';
import { InstructionsTab } from './instructions-tab';
import { AssistantAboutTab } from './assistant/about-tab';
import { RegularChecks } from './assistant/regular-checks';
import { EnvironmentTab } from './environment-tab';
import { IdentityDialog, PeopleDialog } from './profile-dialogs';
import { OwnersDialog } from './owners-dialog';

/** One agent's page: header, ⋯ menu and six tabs (D233). */
export function AgentWorkspace({ agent, directory, back, openChat, busy, section, selectSection, saved, focus }: {
  agent: Agent; directory: Directory; back: () => void; focus?: string;
  openChat: () => void; busy: boolean; section: WorkspaceSection; selectSection: (section: WorkspaceSection) => void;
  saved: () => Promise<void>;
}) {
  const [dialog, setDialog] = useState<AgentAction | 'people'>();
  const [activity, showActivity] = useState(false);
  const mutation = useMutation();
  const choose = (action: AgentAction) => {
    if (action !== 'pause') { setDialog(action); return; }
    const paused = !agent.settings.paused;
    void mutation.run(async () => {
      await saveAgent({ ...agent, people: undefined, settings: { ...agent.settings, paused } });
      await saved();
      mutation.setStatus(paused ? `${agent.name} is paused. It won’t take new requests.` : `${agent.name} is available again.`);
    });
  };
  // A member's own assistant has no instructions box: that tab holds what it
  // knows about its person, its rules and its skills (assistant/about-tab.tsx).
  const instructionsLabel = agent.assistant ? (agent.is_owner ? 'About you' : 'Rules and skills') : 'Instructions';
  const done = async (message: string) => { setDialog(undefined); await saved(); mutation.setStatus(message); };
  return <section className="agent-workspace" aria-label={`${agent.name} workspace`}>
    <AgentWorkspaceHeader agent={agent} busy={busy || mutation.busy} back={back} openChat={openChat} choose={choose} />
    {mutation.status && <p className="agent-page-notice" role="status">{mutation.status}</p>}
    {agent.needs_owner && <p className="agent-page-notice" role="note">{agent.name}’s last owner left the workspace, so it is paused and takes no work.{' '}
      {agent.editable
        ? <Button onClick={() => setDialog('owners')}>Give it an owner</Button>
        : 'An admin needs to give it a new owner.'}</p>}
    <WorkspaceTabs selected={section} select={selectSection} labels={{ Instructions: instructionsLabel }} />
    <div id="agent-workspace-panel" className="agent-workspace-panel" role="tabpanel" aria-label={section === 'Instructions' ? instructionsLabel : section} tabIndex={0}>
      {section === 'Overview' && <AgentOverview agent={agent} open={selectSection} people={() => setDialog('people')} owners={() => setDialog('owners')} />}
      {section === 'Overview' && <section className="agent-overview-activity" aria-label="Activity">
        <header className="agent-section-heading"><h3>Activity</h3>
          <Button aria-expanded={activity} onClick={() => showActivity(shown => !shown)}>{activity ? 'Hide activity' : 'Show activity'}</Button></header>
        {/* Read only when asked for: the list is long and Overview opens often. */}
        {activity && <><p className="agent-filtered-note">Showing {agent.name} only: the same list as Activity, filtered to one agent.{' '}
          <a href={workspaceUrl('/?view=activity&tab=history')}>Open all activity</a></p>
          <HistoryView key={agent.id} agent={agent.id} /></>}</section>}
      {section === 'Instructions' && agent.assistant && <AssistantAboutTab key={agent.id} agent={agent} rules={directory.system_prompt} saved={saved} />}
      {section === 'Instructions' && !agent.assistant && <InstructionsTab key={agent.id} agent={agent} rules={directory.system_prompt} saved={saved} />}
      {section === 'Connections' && <ConnectionsTab key={agent.id} agent={agent} directory={directory} saved={saved} focus={focus} />}
      {section === 'Automations' && agent.assistant && agent.is_owner && <RegularChecks key={agent.id} name={agent.name} />}
      {section === 'Automations' && <AutomationLauncher agent={agent} openChat={openChat} />}
      {section === 'Settings' && <EnvironmentTab key={agent.id} agent={agent} directory={directory} saved={saved} />}
    </div>
    {dialog === 'people' && <PeopleDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
    {dialog === 'identity' && <IdentityDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
    {dialog === 'owners' && <OwnersDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
  </section>;
}
