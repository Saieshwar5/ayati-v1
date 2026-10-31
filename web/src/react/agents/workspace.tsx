import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { loadControls } from './control-model';
import { saveAgent, type Agent, type Directory } from './model';
import { AgentAvatar } from './avatar';
import { AgentWorkspaceHeader, type AgentAction } from './workspace-header';
import { WorkspaceTabs, type WorkspaceSection } from './workspace-tabs';
import { AuthoritySummary } from './authority-summary';
import { AgentWork } from './work';
import { AutomationLauncher } from './automation/launcher';
import { AgentOverview } from './overview';
import { InstructionsTab } from './instructions-tab';
import { EnvironmentTab } from './environment-tab';
import { IdentityDialog, PeopleDialog, TransferDialog } from './profile-dialogs';

/** One agent's page: header, ⋯ menu and six tabs (D233). Tools keeps today's
 * rules until the per-agent rule picker arrives in M3. */
export function AgentWorkspace({ agent, directory, revision, back, guide, control, openChat, busy, section, selectSection, saved }: {
  agent: Agent; directory: Directory; revision: number; back: () => void; guide: () => void; control: () => void;
  openChat: () => void; busy: boolean; section: WorkspaceSection; selectSection: (section: WorkspaceSection) => void;
  saved: () => Promise<void>;
}) {
  const load = useCallback(() => loadControls(agent.id), [agent.id, revision]);
  const controls = useResource(load);
  const [dialog, setDialog] = useState<AgentAction | 'people'>();
  const mutation = useMutation();
  const choose = (action: AgentAction) => {
    if (action !== 'pause') { setDialog(action); return; }
    const paused = !agent.settings.paused;
    void mutation.run(async () => {
      await saveAgent({ ...agent, people: undefined, managers: undefined, settings: { ...agent.settings, paused } });
      await saved();
      mutation.setStatus(paused ? `${agent.name} is paused. It won’t take new requests.` : `${agent.name} is available again.`);
    });
  };
  const done = async (message: string) => { setDialog(undefined); await saved(); mutation.setStatus(message); };
  return <section className="agent-workspace" aria-label={`${agent.name} workspace`}>
    <AgentWorkspaceHeader name={agent.name} title={agent.role} avatar={<AgentAvatar agent={agent} />} paused={agent.settings.paused}
      owner={agent.owner_name} editable={Boolean(agent.editable)} busy={busy || mutation.busy} back={back} openChat={openChat} choose={choose} />
    {mutation.status && <p className="agent-page-notice" role="status">{mutation.status}</p>}
    <WorkspaceTabs selected={section} select={selectSection} />
    <div id="agent-workspace-panel" className="agent-workspace-panel" role="tabpanel" aria-label={section} tabIndex={0}>
      {section === 'Overview' && <AgentOverview agent={agent} open={selectSection} people={() => setDialog('people')} />}
      {section === 'Activity' && <AgentWork agent={agent.id} activity />}
      {section === 'Instructions' && <InstructionsTab key={agent.id} agent={agent} rules={directory.system_prompt} saved={saved} />}
      {section === 'Tools' && <section className="agent-tools" aria-label="Tools">
        <p className="ui-help">Business actions it may take, and which need someone to approve them first.</p>
        <AuthoritySummary data={controls.data} error={controls.error} />
        <div className="agent-profile-shortcuts">
          {agent.editable && <Button aria-label={`Set up permissions for ${agent.name}`} onClick={guide}>Set up permissions</Button>}
          <Button aria-label={`Tools and approvals for ${agent.name}`} onClick={control}>Tools and approvals</Button>
        </div>
      </section>}
      {section === 'Schedules' && <AutomationLauncher agent={agent} />}
      {section === 'Environment' && <EnvironmentTab key={agent.id} agent={agent} directory={directory} saved={saved} />}
    </div>
    {dialog === 'people' && <PeopleDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
    {dialog === 'identity' && <IdentityDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
    {dialog === 'transfer' && <TransferDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
  </section>;
}
