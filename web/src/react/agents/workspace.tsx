import { Button } from '../ui';
import { useState } from 'react';
import { workspaceUrl } from '../../workspace-context';
import { useMutation } from '../requests';
import { ownerNames, saveAgent, type Agent, type Directory } from './model';
import { AgentAvatar } from './avatar';
import { AgentWorkspaceHeader, type AgentAction } from './workspace-header';
import { WorkspaceTabs, type WorkspaceSection } from './workspace-tabs';
import { ToolsTab } from './tools-tab';
import { HistoryView } from '../activity/history';
import '../activity/activity.css';
import { AutomationLauncher } from './automation/launcher';
import { AgentOverview } from './overview';
import { InstructionsTab } from './instructions-tab';
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
  const done = async (message: string) => { setDialog(undefined); await saved(); mutation.setStatus(message); };
  return <section className="agent-workspace" aria-label={`${agent.name} workspace`}>
    <AgentWorkspaceHeader name={agent.name} title={agent.role} avatar={<AgentAvatar agent={agent} />} paused={agent.settings.paused}
      owner={agent.owners ? ownerNames(agent.owners) : agent.owner_name} needsOwner={Boolean(agent.needs_owner)} editable={Boolean(agent.editable)} busy={busy || mutation.busy} back={back} openChat={openChat} choose={choose} />
    {mutation.status && <p className="agent-page-notice" role="status">{mutation.status}</p>}
    {agent.needs_owner && <p className="agent-page-notice" role="note">{agent.name}’s last owner left the workspace, so it is paused and takes no work.{' '}
      {agent.editable
        ? <Button onClick={() => setDialog('owners')}>Give it an owner</Button>
        : 'An admin needs to give it a new owner.'}</p>}
    <WorkspaceTabs selected={section} select={selectSection} />
    <div id="agent-workspace-panel" className="agent-workspace-panel" role="tabpanel" aria-label={section} tabIndex={0}>
      {section === 'Overview' && <AgentOverview agent={agent} open={selectSection} people={() => setDialog('people')} owners={() => setDialog('owners')} />}
      {section === 'Activity' && <>
        <p className="agent-filtered-note">Showing {agent.name} only: the same list as Activity, filtered to one agent.{' '}
          <a href={workspaceUrl('/?view=activity&tab=history')}>Open all activity</a></p>
        <HistoryView key={agent.id} agent={agent.id} /></>}
      {section === 'Instructions' && <InstructionsTab key={agent.id} agent={agent} rules={directory.system_prompt} saved={saved} />}
      {section === 'Tools' && <ToolsTab key={agent.id} agent={agent} webSearch={Boolean(directory.web_search)} computers={Boolean(directory.computers)} saved={saved} focus={focus} />}
      {section === 'Automations' && <AutomationLauncher agent={agent} openChat={openChat} />}
      {section === 'Environment' && <EnvironmentTab key={agent.id} agent={agent} directory={directory} saved={saved} />}
    </div>
    {dialog === 'people' && <PeopleDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
    {dialog === 'identity' && <IdentityDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
    {dialog === 'owners' && <OwnersDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
  </section>;
}
