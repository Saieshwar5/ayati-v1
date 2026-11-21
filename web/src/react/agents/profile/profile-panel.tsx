import { useState, type ReactNode } from 'react';
import { Button } from '../../ui';
import { useMutation } from '../../requests';
import { AlwaysRead } from '../../library/always-read';
import { AssistantMemory } from '../assistant/memory';
import { RegularChecks } from '../assistant/regular-checks';
import { AutomationLauncher } from '../automation/launcher';
import { ConnectionsTab } from '../connections-tab';
import { EnvironmentTab } from '../environment-tab';
import { Knowledge } from '../knowledge';
import { LearnedLessons } from '../learned-lessons';
import { saveAgent, type Agent, type Directory } from '../model';
import { AgentSkillChat } from '../skills/agent-skill-chat';
import { ProfileIcon } from './profile-parts';
import type { ProfilePanel } from './profile-rows';

const titles: Record<ProfilePanel['kind'], string> = {
  assistant: 'Change with chat', notes: 'Notes', documents: 'Documents', lessons: 'Lessons from its work', memory: 'About you',
  app: 'Can use', connections: 'Can use', automations: 'On its own', checks: 'Regular checks', model: 'Model',
};

/**
 * The right-hand column while something on the page is open: the editor for
 * that one thing, in place of Recent work. Each editor is the one the app
 * already had; `changed` asks the page to read its rows again.
 */
export function ProfilePanelView({ agent, directory, panel, close, changed, saved, openChat, takeRules }: {
  agent: Agent; directory: Directory; panel: ProfilePanel; close: () => void; changed: () => void;
  saved: () => Promise<void>; openChat: () => void;
  /** The assistant's proposed rules go to the page's own rules box. */
  takeRules: (text: string) => void;
}) {
  let body: ReactNode;
  switch (panel.kind) {
    case 'assistant': body = <AgentSkillChat agent={agent} rules={agent.settings.instructions} takeRules={agent.assistant ? undefined : takeRules} changed={changed} />; break;
    case 'notes': body = <><NotesSwitch agent={agent} saved={saved} /><Knowledge key={String(panel.adding)} agent={agent} adding={panel.adding} changed={changed} /><HistoryChoice agent={agent} saved={saved} /></>; break;
    case 'documents': body = <AlwaysRead agent={agent} changed={changed} />; break;
    case 'lessons': body = <LearnedLessons agent={agent} saved={async () => { changed(); }} />; break;
    case 'memory': body = <AssistantMemory name={agent.name} />; break;
    case 'app': body = <ConnectionsTab key={panel.id} agent={agent} directory={directory} saved={saved} focus={panel.focus} part={{ app: panel.id }} changed={changed} />; break;
    case 'connections': body = <ConnectionsTab key={panel.part ?? 'all'} agent={agent} directory={directory} saved={saved} part={panel.part} start={panel.start} changed={changed} />; break;
    case 'automations': body = <AutomationLauncher agent={agent} openChat={openChat} start={panel.start} changed={changed} />; break;
    case 'checks': body = <RegularChecks name={agent.name} />; break;
    case 'model': body = <EnvironmentTab agent={agent} directory={directory} saved={saved} />; break;
  }
  return <section className="profile-panel" aria-label={titles[panel.kind]}>
    <header><h3>{titles[panel.kind]}</h3>
      <Button className="ui-icon profile-panel-close" aria-label="Close" onClick={close}><ProfileIcon name="x" size={18} /></Button></header>
    <div className="profile-panel-body">{body}</div>
  </section>;
}

/** Whether the agent uses its approved notes at all. */
function NotesSwitch({ agent, saved }: { agent: Agent; saved: () => Promise<void> }) {
  const mutation = useMutation();
  if (!agent.editable) return null;
  const flip = (memory: boolean) => void mutation.run(async () => {
    await saveAgent({ ...agent, people: undefined, settings: { ...agent.settings, memory } });
    await saved();
  });
  return <div className="profile-switch"><span>Use notes in its answers</span>
    <label className="ui-switch"><input type="checkbox" role="switch" checked={agent.settings.memory} disabled={mutation.busy}
      onChange={event => flip(event.target.checked)} /><span>Use notes in its answers</span></label>
    {mutation.status && <small role="alert">{mutation.status}</small>}</div>;
}

/** How much of a channel's earlier messages it reads. */
function HistoryChoice({ agent, saved }: { agent: Agent; saved: () => Promise<void> }) {
  const mutation = useMutation();
  const [open, setOpen] = useState(false);
  if (!agent.editable || agent.assistant) return null;
  const choose = (history: string) => void mutation.run(async () => {
    await saveAgent({ ...agent, people: undefined, settings: { ...agent.settings, history } });
    await saved();
  });
  return <details className="agent-advanced" open={open} onToggle={event => setOpen(event.currentTarget.open)}><summary>Channel history</summary>
    <label className="ui-field">Channel history<select disabled={mutation.busy} value={agent.settings.history} onChange={event => choose(event.target.value)}>
      <option value="all">Recent history, including before it joined</option>
      <option value="since_join">Only messages since it joined</option>
    </select></label>
    {mutation.status && <p role="alert">{mutation.status}</p>}
  </details>;
}
