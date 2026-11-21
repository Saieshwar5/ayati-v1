import { useEffect, useRef, useState } from 'react';
import { useSkillWriting } from '../skills/setup-chat';
import { assignSkill } from '../skills/model';
import { SkillView } from '../skills/agent-skills';
import { SkillPicker } from '../skills/skill-picker';
import { CatalogDialog } from '../../apps/catalog-dialog';
import { ConnectDialog } from '../../apps/connect-dialog';
import type { CatalogApp } from '../../apps/apps-model';
import { useMutation } from '../../requests';
import { Button } from '../../ui';
import type { Agent, Directory } from '../model';
import { OwnersDialog } from '../owners-dialog';
import { PeopleDialog } from '../profile-dialogs';
import { EverythingItReceives } from '../system-prompt';
import { CanUseRow, DoesRow, KnowsRow, ModelRow, OnItsOwnRow, PeopleRow, type ProfileDialog, type ProfilePanel } from './profile-rows';
import { ProfilePanelView } from './profile-panel';
import { RecentWork } from './recent-work';
import { useAgentSummary } from './use-agent-summary';
import './profile.css';

/**
 * One agent's page (designs/agents-profile, 2026-11-21): no tabs. Six rows show
 * the real things (its rules, skills, what it knows, what it can use, what it
 * does on its own, its people and model). Clicking a thing opens it and ＋ adds
 * to the row it is in. The right-hand column is Recent work until something is
 * opened, then the editor for that one thing.
 */
export function AgentProfilePage({ agent, directory, start, openChat, saved, notice }: {
  agent: Agent; directory: Directory;
  /** A link may open one panel (workspace-tabs.tsx: agentSectionUrl). */
  start?: ProfilePanel; openChat: () => void; saved: () => Promise<void>; notice: (message: string) => void;
}) {
  const summary = useAgentSummary(agent);
  const [panel, setPanel] = useState<ProfilePanel | undefined>(start);
  const [dialog, setDialog] = useState<ProfileDialog>();
  const [draft, setDraft] = useState<string>();
  const [connecting, setConnecting] = useState<CatalogApp>();
  const writing = useSkillWriting();
  const skills = useMutation();
  const side = useRef<HTMLElement>(null);
  const changed = () => void summary.refresh().catch(() => {});
  const close = () => { setPanel(undefined); changed(); };
  // A panel that opens takes the keyboard to its column, where its content is.
  const kind = panel?.kind, part = panel && 'part' in panel ? panel.part : panel && 'id' in panel ? panel.id : undefined;
  useEffect(() => { if (kind) side.current?.querySelector<HTMLElement>('.profile-panel-close')?.focus({ preventScroll: true }); }, [kind, part]);
  const done = async (message: string) => { setDialog(undefined); await saved(); notice(message); };
  const rows = { agent, directory, summary: summary.data, panel, open: setPanel, show: setDialog };
  // Your assistant where it can work, else the skill writer, which answers as the agent itself.
  const helper = agent.editable && !writing.loading && writing.available
    ? writing.helper ?? { id: agent.id, name: agent.name, avatar: agent.settings.avatar } : undefined;
  const chosen = summary.data?.skills ?? [];
  return <div className="profile-page">
    <div className="profile-main">
      <DoesRow {...rows} draft={draft} setDraft={setDraft} saved={saved} helper={helper} />
      <KnowsRow {...rows} />
      <CanUseRow {...rows} />
      <OnItsOwnRow {...rows} />
      <PeopleRow {...rows} />
      <ModelRow {...rows} />
      {skills.status && <p role="alert">{skills.status}</p>}
    </div>
    <aside ref={side} className={`profile-side${panel ? ' has-panel' : ''}`} aria-label={panel ? 'Details' : 'Recent work'}>
      {panel ? <ProfilePanelView agent={agent} directory={directory} panel={panel} close={close} changed={changed} saved={saved} openChat={openChat}
        takeRules={text => setDraft(text)} />
        : <RecentWork requests={summary.data?.requests ?? []} all={Boolean(agent.editable)} loaded={Boolean(summary.data)} />}
    </aside>
    {dialog?.kind === 'skill' && <SkillView id={dialog.skill.id} close={() => setDialog(undefined)}
      footer={agent.editable ? <Button className="danger" disabled={skills.busy} onClick={() => void skills.run(async () => { await assignSkill(agent.id, dialog.skill.id, false); setDialog(undefined); changed(); })}>Remove from {agent.name}</Button> : undefined} />}
    {dialog?.kind === 'add-skill' && <SkillPicker agent={agent.name} owners={agent.owners?.map(owner => owner.id) ?? (agent.owner_id ? [agent.owner_id] : [])}
      chosen={chosen.map(skill => skill.id)} close={() => { setDialog(undefined); changed(); }}
      choose={async skill => { await assignSkill(agent.id, skill.id, true); await summary.refresh(); }} />}
    {dialog?.kind === 'add-app' && !connecting && <CatalogDialog agent={agent.id} close={() => setDialog(undefined)} connect={setConnecting} />}
    {connecting && <ConnectDialog app={connecting} agent={agent} close={() => { setConnecting(undefined); setDialog(undefined); }}
      connected={() => { setConnecting(undefined); setDialog(undefined); changed(); }} />}
    {dialog?.kind === 'people' && <PeopleDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
    {dialog?.kind === 'owners' && <OwnersDialog agent={agent} close={() => setDialog(undefined)} saved={done} />}
    {dialog?.kind === 'receives' && <EverythingItReceives draft={{ name: agent.name, role: agent.role, instructions: draft ?? agent.settings.instructions, agent: agent.id }} close={() => setDialog(undefined)} />}
  </div>;
}
