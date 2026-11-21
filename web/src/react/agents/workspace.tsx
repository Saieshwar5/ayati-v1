import { Button } from '../ui';
import { useState } from 'react';
import { useMutation } from '../requests';
import { saveAgent, type Agent, type Directory } from './model';
import { AgentWorkspaceHeader, type AgentAction } from './workspace-header';
import { IdentityDialog } from './profile-dialogs';
import { OwnersDialog } from './owners-dialog';
import { AgentProfilePage } from './profile/profile-page';
import type { ProfilePanel } from './profile/profile-rows';

/** One agent's page: its header, then one page of rows with Recent work beside
 * them (profile/profile-page.tsx). `start` is a panel a link asked for. */
export function AgentWorkspace({ agent, directory, back, openChat, busy, start, saved }: {
  agent: Agent; directory: Directory; back: () => void; start?: ProfilePanel;
  openChat: () => void; busy: boolean; saved: () => Promise<void>;
}) {
  const [dialog, setDialog] = useState<AgentAction>();
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
    <AgentWorkspaceHeader agent={agent} busy={busy || mutation.busy} back={back} openChat={openChat} choose={choose} />
    {mutation.status && <p className="agent-page-notice" role="status">{mutation.status}</p>}
    {agent.needs_owner && <p className="agent-page-notice" role="note">{agent.name}’s last owner left the workspace, so it is paused and takes no work.{' '}
      {agent.editable
        ? <Button onClick={() => setDialog('owners')}>Give it an owner</Button>
        : 'An admin needs to give it a new owner.'}</p>}
    <AgentProfilePage key={agent.id} agent={agent} directory={directory} start={start} openChat={openChat} saved={saved} notice={mutation.setStatus} />
    {dialog === 'identity' && <IdentityDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
    {dialog === 'owners' && <OwnersDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
  </section>;
}
