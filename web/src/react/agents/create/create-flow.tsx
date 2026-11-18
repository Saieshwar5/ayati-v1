import { useRef, useState } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { currentWorkspace, workspaceSession } from '../../../workspace-context';
import { defaultRobot } from '../../faces';
import { defaults, saveAgentWithPicture, type Agent, type Audience, type Directory } from '../model';
import { NameStep } from './name-step';
import { SkillsStep } from './skills-step';
import { loadMembers, PeopleStep } from './people-step';
import { assignSkill, createSkill } from '../skills/model';
import { useSkillWriting } from '../skills/setup-chat';
import { skillText, type DraftSkill, type LibraryPick } from '../skills/writer-model';
import { SkillPicker } from '../skills/skill-picker';
import { SkillIcon } from '../skills/skill-icons';
import { computerDefaults } from '../computer/model';
import './create.css';

const steps = ['Name and picture', 'What it does', 'People'];
const hints = [
  'Only the name is needed. Everything else can change later.',
  'Ground rules and skills can be changed at any time.',
  'After it is created, add its apps and MCP servers on its Connections tab.',
];

/** A fresh agent draft owned by the person creating it. */
export function newAgent(directory: Directory): Agent {
  const id = crypto.randomUUID();
  // The workspace's default model, with its provider when the admin listed models.
  // A catalog model never names a provider: Cuckoo chooses who serves it.
  const provider = directory.catalog?.catalog_on ? ''
    : directory.models?.find(choice => choice.model === directory.default_model)?.provider ?? defaults.provider;
  // Every new agent has its own computer and browser where this Cuckoo offers
  // them, with the internet on; the workspace's limits still apply.
  const computer = directory.computers ? { ...computerDefaults, on: true, internet: 'any' as const } : undefined;
  return { id, name: '', role: '', model: directory.default_model, audience: 'everyone', revision: 0,
    settings: { ...defaults, provider, avatar: defaultRobot(id), ...(computer ? { computer } : {}) }, editable: true };
}

/** Create an agent in three short steps: name and picture, what it does (its
 * ground rules and skills, designs/agent-built-skills), and who can talk to it.
 * Everything else starts with a default and changes later on the agent's page:
 * web search and its computer are on where this Cuckoo offers them, the model
 * is the workspace's, and apps and MCP servers are added on its Connections
 * tab, where each belongs to this agent alone (schema 132). Retrying after a
 * failure updates the same agent. */
export function AgentCreate({ start, close, created }: {
  start: Agent; close: () => void; created: (id: string) => Promise<void>;
}) {
  const me = workspaceSession?.user?.id ?? '';
  const [draft, setDraft] = useState(start);
  const [file, setFile] = useState<File>();
  const [step, setStep] = useState(0);
  const [audience, setAudience] = useState<Audience>(start.audience);
  const [people, setPeople] = useState<string[]>([]);
  const members = useResource(loadMembers);
  const [skills, setSkills] = useState<LibraryPick[]>([]);
  const [drafts, setDrafts] = useState<DraftSkill[]>([]);
  const writing = useSkillWriting();
  const addSkills = (picks: LibraryPick[]) => setSkills(old => [...old, ...picks.filter(pick => !old.some(skill => skill.id === pick.id))]);
  const [pickingSkills, setPickingSkills] = useState(false);
  const mutation = useMutation();
  const first = useRef<HTMLInputElement>(null);
  const ready = [Boolean(draft.name.trim()), Boolean(draft.settings.instructions.trim()), true];
  const submit = () => mutation.run(async () => {
    const input = { ...draft, audience, people: audience === 'selected' ? people : [] };
    // A saved agent keeps its revision if the picture upload fails, so a retry updates it.
    await saveAgentWithPicture(input, file, next => setDraft(old => ({ ...old, revision: next.revision, settings: next.settings })));
    setFile(undefined);
    // Written skills become the owner's own skills. Saving and assigning are
    // idempotent, so a retry after a failure saves and adds each one once.
    for (const written of drafts) {
      const saved = await createSkill(written.id, false, [{ path: 'SKILL.md', text: skillText(written.name, written.description, written.steps) }]);
      await assignSkill(draft.id, saved.id, true);
    }
    for (const skill of skills) await assignSkill(draft.id, skill.id, true);
    await created(draft.id);
  });
  return <section className="agent-create" aria-labelledby="agent-create-title">
    <header className="agent-create-header">
      <h2 id="agent-create-title">New agent</h2>
      <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
    </header>
    <ol className="agent-create-steps" aria-label="Steps">{steps.map((title, index) =>
      <li key={title} aria-current={index === step ? 'step' : undefined} className={index < step ? 'done' : undefined}>
        <span aria-hidden="true">{index < step ? '✓' : index + 1}</span>{title}</li>)}</ol>
    <fieldset className="ui-fieldset agent-create-body" disabled={mutation.busy}>
      {step === 0 && <NameStep draft={draft} file={file} first={first}
        change={(key, value) => setDraft(old => ({ ...old, [key]: value }))}
        choose={robot => { setFile(undefined); setDraft(old => ({ ...old, settings: { ...old.settings, avatar: robot } })); }}
        upload={setFile} reject={mutation.setStatus} />}
      {step === 1 && <SkillsStep name={draft.name} title={draft.role} text={draft.settings.instructions}
        apps={false} connections={[]} writing={writing} drafts={drafts} setDrafts={setDrafts} library={skills} reuse={addSkills}
        change={(text, title) => setDraft(old => ({ ...old, role: title ?? old.role, settings: { ...old.settings, instructions: text } }))}>
        <div className="skill-chips">{skills.map(skill => <span key={skill.id} className="skill-chip"><SkillIcon name="book" />{skill.name}
          <button type="button" aria-label={`Remove ${skill.name}`} onClick={() => setSkills(old => old.filter(s => s.id !== skill.id))}>×</button></span>)}
          <Button onClick={() => setPickingSkills(true)}>＋ Add skills</Button></div>
      </SkillsStep>}
      {step === 2 && <PeopleStep name={draft.name} workspace={currentWorkspace()?.name ?? 'this workspace'} me={me}
        members={members.data?.users} audience={audience} people={people}
        setAudience={setAudience} setPeople={setPeople} />}
    </fieldset>
    {pickingSkills && <SkillPicker agent={draft.name} owners={[me]} chosen={skills.map(s => s.id)} close={() => setPickingSkills(false)}
      choose={({ id, name, description }) => addSkills([{ id, name, description }])} />}
    <footer className="agent-create-footer">
      {step > 0 && <Button disabled={mutation.busy} onClick={() => setStep(step - 1)}>Back</Button>}
      {step < steps.length - 1
        ? <Button primary disabled={!ready[step]} onClick={() => { mutation.setStatus(''); setStep(step + 1); }}>Continue</Button>
        : <Button primary disabled={mutation.busy || !ready.every(Boolean)} onClick={() => void submit()}>Create agent</Button>}
      <p className="ui-help">{hints[step]}</p>
      <p className="ui-status" role="status">{mutation.busy ? 'Creating…' : mutation.status}</p>
    </footer>
  </section>;
}
