import { useState } from 'react';
import { AgentFace, PersonFace } from '../../faces';
import { useMutation } from '../../requests';
import { AppLetter, stateWords } from '../../apps/app-marks';
import { appState } from '../../apps/apps-model';
import { workspaceSession, workspaceUrl } from '../../../workspace-context';
import { connectorName } from '../app-connections';
import { cadence, stateLabels } from '../automation/model';
import { computerOf } from '../computer/model';
import { starters } from '../create/starters';
import { saveAgent, type Agent, type Directory } from '../model';
import type { Skill } from '../skills/model';
import { ContextPreviewButton } from '../knowledge';
import { searchesWeb } from '../web-search';
import { AssistantRules } from '../assistant/rules';
import { AddWords, Chip, Plus, ProfileIcon, Row, type PlusItem } from './profile-parts';
import { RulesEditor } from './rules-editor';
import type { AgentSummary } from './use-agent-summary';

/** What the right-hand column shows in place of Recent work. */
export type ProfilePanel =
  | { kind: 'assistant' } | { kind: 'notes'; adding?: boolean } | { kind: 'documents' } | { kind: 'lessons' } | { kind: 'memory' }
  | { kind: 'agent-memory' } | { kind: 'about-me' } | { kind: 'improving' }
  | { kind: 'app'; id: string; focus?: string } | { kind: 'connections'; part?: 'servers' | 'computers' | 'web'; start?: boolean }
  | { kind: 'automations'; start?: boolean } | { kind: 'checks' } | { kind: 'model' };
/** Dialogs the rows open: they already exist and stay dialogs. */
export type ProfileDialog = { kind: 'skill'; skill: Skill } | { kind: 'add-skill' } | { kind: 'add-app' } | { kind: 'people' } | { kind: 'owners' } | { kind: 'receives' };

export type RowProps = {
  agent: Agent; directory: Directory; summary?: AgentSummary; panel?: ProfilePanel;
  open: (panel: ProfilePanel) => void; show: (dialog: ProfileDialog) => void;
};
const is = (panel: ProfilePanel | undefined, kind: ProfilePanel['kind'], part?: string) =>
  panel?.kind === kind && (part === undefined || ('part' in panel && panel.part === part) || ('id' in panel && panel.id === part));
const dot = (state: 'ok' | 'warn' | 'off') => <span className={`profile-dot ${state}`} aria-hidden="true" />;

/** Does: the ground rules as text, edited in place, then the skills. */
export function DoesRow({ agent, summary, panel, open, show, draft, setDraft, saved, helper }: RowProps & {
  /** The rules box is open with this text; undefined while the saved rules are read. */
  draft?: string; setDraft: (text?: string) => void; saved: () => Promise<void>;
  /** Who proposes changes by chat here: the viewer's assistant, or the agent's own skill writer. */
  helper?: { id: string; name: string; avatar?: string | null };
}) {
  const edit = Boolean(agent.editable), rules = agent.settings.instructions.trim();
  const skills = summary?.skills ?? [];
  return <Row icon="rules" label={agent.assistant ? 'Rules' : 'Does'}
    action={edit && !agent.assistant && rules && draft === undefined
      ? <button type="button" className="profile-edit" aria-label="Edit the rules" onClick={() => setDraft(rules)}><ProfileIcon name="pencil" /></button> : undefined}>
    {agent.assistant ? <AssistantRulesHere agent={agent} saved={saved} />
      : draft !== undefined ? <RulesEditor agent={agent} start={draft} close={() => setDraft(undefined)} saved={saved} />
        : rules ? (edit ? <button type="button" className="profile-rules" aria-label="Edit the rules" onClick={() => setDraft(rules)}>{rules}</button> : <p className="profile-rules">{rules}</p>)
          : edit ? <div className="profile-chips profile-start">
            <button type="button" className="profile-chip profile-add" onClick={() => setDraft('')}><ProfileIcon name="plus" size={14} />Write rules</button>
            <span className="profile-quiet">or start from</span>
            {starters.filter(starter => starter.text(agent.name).trim()).map(starter => <button key={starter.id} type="button" className="profile-chip profile-pick" onClick={() => setDraft(starter.text(agent.name))}>{starter.label}</button>)}
          </div> : <p className="profile-quiet">No rules yet</p>}
    <div className="profile-chips">
      {skills.map(skill => <Chip key={skill.id} open={() => show({ kind: 'skill', skill })}><ProfileIcon name="skill" size={14} />{skill.name}
        {skill.created_by_agent && <small className="profile-made" title={`Created by ${skill.created_by_agent}`}>by {skill.created_by_agent}</small>}</Chip>)}
      {edit && <Plus label="Add a skill" word={skills.length ? undefined : 'Skill'} items={[{ icon: 'skill', label: 'Skill', pick: () => show({ kind: 'add-skill' }) }]} />}
      {edit && helper && <button type="button" className="profile-chip profile-pick" aria-current={is(panel, 'assistant') ? 'true' : undefined} onClick={() => open({ kind: 'assistant' })}>
        <AgentFace agent={{ id: helper.id, name: helper.name, avatar: helper.avatar ?? undefined }} className="chat-avatar profile-mini-face" />Ask {helper.name}</button>}
      <button type="button" className="profile-lock" onClick={() => show({ kind: 'receives' })}><ProfileIcon name="lock" size={13} />Cuckoo’s rules</button>
    </div>
  </Row>;
}

/** An assistant's rules are a short list kept as its instructions, one per line. */
function AssistantRulesHere({ agent, saved }: { agent: Agent; saved: () => Promise<void> }) {
  // The revision the list was last saved at, so a save never overwrites a newer change.
  const [base, setBase] = useState(agent);
  const mutation = useMutation();
  if (base.revision < agent.revision) setBase(agent);
  const save = async (next: string, done: string) => {
    let kept = false;
    await mutation.run(async () => {
      const settings = { ...base.settings, instructions: next };
      const result = await saveAgent({ ...base, people: undefined, settings });
      setBase({ ...base, revision: result.revision, settings });
      kept = true; mutation.setStatus(done);
      await saved();
    });
    return kept;
  };
  return <AssistantRules text={base.settings.instructions} editable={Boolean(agent.editable)} busy={mutation.busy} status={mutation.status} save={save} />;
}

/** Knows: notes, the documents it always reads, lessons from its work, its memory
 * (owners and admins), and what it remembers about you (your own assistant, or any
 * shared agent you talk to). */
export function KnowsRow({ agent, summary, panel, open }: RowProps) {
  const edit = Boolean(agent.editable);
  const notes = summary?.notes ?? 0, documents = summary?.documents ?? 0, lessons = summary?.lessons;
  const add: PlusItem[] = [{ icon: 'note', label: 'Note', pick: () => open({ kind: 'notes', adding: true }) }, { icon: 'doc', label: 'Document', pick: () => open({ kind: 'documents' }) }];
  const any = notes > 0 || documents > 0 || Boolean(lessons?.available) || Boolean(agent.assistant && agent.is_owner) || !agent.assistant;
  return <Row icon="book" label="Knows"><div className="profile-chips">
    {agent.assistant && agent.is_owner && <Chip open={() => open({ kind: 'memory' })} current={is(panel, 'memory')}><ProfileIcon name="memory" size={14} />About you</Chip>}
    {notes > 0 && <Chip open={() => open({ kind: 'notes' })} current={is(panel, 'notes')}><ProfileIcon name="note" size={14} />Notes <small>{notes}</small></Chip>}
    {documents > 0 && <Chip open={() => open({ kind: 'documents' })} current={is(panel, 'documents')}><ProfileIcon name="doc" size={14} />Documents <small>{documents}</small></Chip>}
    {edit && lessons?.available && <Chip open={() => open({ kind: 'lessons' })} current={is(panel, 'lessons')}><ProfileIcon name="bulb" size={14} />Lessons
      {lessons.waiting > 0 && <span className="profile-pill">{lessons.waiting} to review</span>}</Chip>}
    {!agent.assistant && edit && <Chip open={() => open({ kind: 'agent-memory' })} current={is(panel, 'agent-memory')}><ProfileIcon name="bulb" size={14} />Memory</Chip>}
    {!agent.assistant && edit && <Chip open={() => open({ kind: 'improving' })} current={is(panel, 'improving')}><ProfileIcon name="bolt" size={14} />Improving</Chip>}
    {!agent.assistant && <Chip open={() => open({ kind: 'about-me' })} current={is(panel, 'about-me')}><ProfileIcon name="memory" size={14} />About you</Chip>}
    {edit ? (notes || documents ? <Plus label="Add a note or a document" items={add} /> : <AddWords items={add} />) : !any && <span className="profile-quiet">Nothing yet</span>}
    {!agent.assistant && <ContextPreviewButton agent={agent} quiet />}
  </div></Row>;
}

/** Can use: each app, MCP server and computer with its state, and web search. */
export function CanUseRow({ agent, directory, summary, panel, open, show }: RowProps) {
  const edit = Boolean(agent.editable);
  const apps = summary?.apps ?? [], servers = summary?.servers ?? [];
  const cuckooComputer = Boolean(directory.computers) && computerOf(agent.settings).on;
  const web = Boolean(directory.web_search) && searchesWeb(agent.settings);
  const add: PlusItem[] = [{ icon: 'plug', label: 'App', pick: () => show({ kind: 'add-app' }) }];
  if (directory.computers || summary?.canAddComputer) add.push({ icon: 'monitor', label: 'Computer', pick: () => open({ kind: 'connections', part: 'computers' }) });
  add.push({ icon: 'server', label: 'MCP server', pick: () => open({ kind: 'connections', part: 'servers', start: true }) });
  if (directory.web_search && !web) add.push({ icon: 'globe', label: 'Web search', pick: () => open({ kind: 'connections', part: 'web' }) });
  const any = apps.length || servers.length || cuckooComputer || summary?.ownComputer || web;
  return <Row icon="plug" label="Can use"><div className="profile-chips">
    {apps.map(app => { const state = appState(app); return <Chip key={app.id} open={() => open({ kind: 'app', id: app.id })} current={is(panel, 'app', app.id)}>
      <AppLetter id={app.server} name={connectorName(app)} size="small" />{connectorName(app)}{dot(state === 'ok' ? 'ok' : 'warn')}{state !== 'ok' && <small className="profile-warn">{stateWords[state]}</small>}</Chip>; })}
    {servers.map(server => <Chip key={server.id} open={() => open({ kind: 'connections', part: 'servers' })} current={is(panel, 'connections', 'servers')}>
      <span className="profile-tile"><ProfileIcon name="server" size={13} /></span>{connectorName(server)}{dot(server.on ? 'ok' : 'off')}</Chip>)}
    {cuckooComputer && <Chip open={() => open({ kind: 'connections', part: 'computers' })} current={is(panel, 'connections', 'computers')}>
      <span className="profile-tile"><ProfileIcon name="monitor" size={13} /></span>Computer{dot('ok')}</Chip>}
    {summary?.ownComputer && <Chip open={() => open({ kind: 'connections', part: 'computers' })} current={is(panel, 'connections', 'computers')}>
      <span className="profile-tile"><ProfileIcon name="monitor" size={13} /></span>{summary.ownComputer.name}{dot(summary.ownComputer.state === 'online' ? 'ok' : 'off')}</Chip>}
    {web && <Chip open={() => open({ kind: 'connections', part: 'web' })} current={is(panel, 'connections', 'web')}><span className="profile-tile"><ProfileIcon name="globe" size={13} /></span>Web search</Chip>}
    {edit ? (any ? <Plus label="Add an app, computer or MCP server" items={add} /> : <AddWords items={add.slice(0, 3)} />) : !any && <span className="profile-quiet">Nothing yet</span>}
  </div></Row>;
}

const soon = (ms: number) => {
  const date = new Date(ms), day = date.toDateString();
  const tomorrow = new Date(Date.now() + 86400000).toDateString();
  const at = date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  return day === new Date().toDateString() ? `today ${at}` : day === tomorrow ? `tomorrow ${at}` : date.toLocaleDateString([], { day: 'numeric', month: 'short' });
};

/** On its own: every schedule and trigger as a line with when it runs next. */
export function OnItsOwnRow({ agent, summary, panel, open }: RowProps) {
  const edit = Boolean(agent.editable);
  const schedules = summary?.schedules ?? [], triggers = summary?.triggers ?? [];
  const current = is(panel, 'automations') ? 'true' : undefined;
  const own = agent.assistant && agent.is_owner;
  return <Row icon="clock" label="On its own">
    {schedules.map(routine => <button key={routine.id} type="button" className={`profile-line${routine.state === 'enabled' ? '' : ' is-off'}`} aria-current={current} onClick={() => open({ kind: 'automations' })}>
      <span className="profile-tile round"><ProfileIcon name="clock" size={14} /></span><span><strong>{routine.name}</strong> · {cadence(routine.interval_seconds)}</span>
      <small>{routine.state !== 'enabled' ? stateLabels[routine.state] : routine.next_ms ? `next · ${soon(routine.next_ms)}` : ''}</small></button>)}
    {triggers.map(trigger => <button key={trigger.id} type="button" className={`profile-line${trigger.state === 'enabled' ? '' : ' is-off'}`} aria-current={current} onClick={() => open({ kind: 'automations' })}>
      <span className="profile-tile round"><ProfileIcon name="bolt" size={14} /></span><span><strong>{trigger.name}</strong> · {trigger.source_definition?.name ?? 'Trigger'}</span>
      <small>{trigger.state === 'enabled' ? '' : trigger.state === 'paused' ? 'Paused' : 'Off'}</small></button>)}
    <div className="profile-chips">
      {own && <Chip open={() => open({ kind: 'checks' })} current={is(panel, 'checks')}><ProfileIcon name="check" size={14} />Regular checks</Chip>}
      {edit ? <Plus label="Add a schedule or trigger" word={schedules.length || triggers.length ? undefined : 'Schedule or trigger'}
        items={[{ icon: 'clock', label: 'Schedule or trigger', pick: () => open({ kind: 'automations', start: true }) }]} />
        : <a className="profile-chip" href={workspaceUrl('/?view=activity&tab=upcoming')}><ProfileIcon name="clock" size={14} />Upcoming<ProfileIcon name="out" size={13} /></a>}
    </div>
  </Row>;
}

/** People: its owners, and who can message it. */
export function PeopleRow({ agent, show }: RowProps) {
  const me = workspaceSession?.user?.id;
  const owners = agent.owners ?? (agent.owner_id && agent.owner_name ? [{ id: agent.owner_id, name: agent.owner_name }] : []);
  if (agent.assistant) return <Row icon="people" label="People"><div className="profile-chips">
    <Chip><ProfileIcon name="lock" size={14} />Only {agent.is_owner ? 'you' : agent.owner_name ?? 'its person'}</Chip></div></Row>;
  const edit = Boolean(agent.editable);
  const audience = agent.audience === 'everyone' ? 'Everyone can message it' : agent.audience === 'selected' ? 'Selected people can message it' : 'Only its owners can message it';
  return <Row icon="people" label="People"><div className="profile-chips">
    {agent.needs_owner && <Chip open={edit ? () => show({ kind: 'owners' }) : undefined}><ProfileIcon name="ask" size={14} />No owner</Chip>}
    {owners.map(owner => <Chip key={owner.id} open={edit ? () => show({ kind: 'owners' }) : undefined} label={edit ? `Owners: ${owner.id === me ? 'You' : owner.name}` : undefined}>
      <PersonFace person={owner} className="chat-avatar profile-mini-face" />{owner.id === me ? 'You' : owner.name} <small>owner</small></Chip>)}
    <Chip open={edit ? () => show({ kind: 'people' }) : undefined}><ProfileIcon name="people" size={14} />{audience}</Chip>
  </div></Row>;
}

/** Model: what it runs on; its owners and admins open the choice, its usage and budgets. */
export function ModelRow({ agent, directory, panel, open }: RowProps) {
  const name = directory.catalog?.models.find(model => model.id === agent.model)?.name ?? agent.model;
  return <Row icon="chip" label="Model"><div className="profile-chips">
    <Chip open={agent.editable ? () => open({ kind: 'model' }) : undefined} current={is(panel, 'model')}><ProfileIcon name="chip" size={14} />{name}</Chip>
  </div></Row>;
}
