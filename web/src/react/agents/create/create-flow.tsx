import { useRef, useState } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { currentWorkspace, workspaceSession } from '../../../workspace-context';
import { defaultRobot } from '../../faces';
import { defaults, saveAgentWithPicture, type Agent, type Audience, type Directory } from '../model';
import { NameStep } from './name-step';
import { InstructionsStep } from './instructions-step';
import { loadMembers, PeopleStep } from './people-step';
import { ToolsStep, type ToolChoices } from './tools-step';
import { loadAvailable, saveRule, switchTool } from '../control-model';
import { assignSkill, type Skill } from '../skills/model';
import { SkillPicker } from '../skills/skill-picker';
import './create.css';

const steps = ['Name and picture', 'Instructions', 'People', 'Tools and safety'];
const hints = [
  'Only the name is needed. Everything else can change later.',
  'Instructions can be edited at any time.',
  'Defaults are fine for most agents.',
  'Connectors start off. Tools can change later on the Tools tab.',
];

/** A fresh agent draft owned by the person creating it. */
export function newAgent(directory: Directory): Agent {
  const id = crypto.randomUUID();
  // The workspace's default model, with its provider when the admin listed models.
  const provider = directory.models?.find(choice => choice.model === directory.default_model)?.provider ?? defaults.provider;
  return { id, name: '', role: '', model: directory.default_model, audience: 'everyone', revision: 0,
    settings: { ...defaults, provider, avatar: defaultRobot(id) }, editable: true };
}

/** Create an agent in four short steps (D233). Tools are saved right after the
 * agent; retrying after a failure updates the same agent and its tools. */
export function AgentCreate({ start, directory, close, created }: {
  start: Agent; directory: Directory; close: () => void; created: (id: string) => Promise<void>;
}) {
  const me = workspaceSession?.user?.id ?? '';
  const [draft, setDraft] = useState(start);
  const [file, setFile] = useState<File>();
  const [step, setStep] = useState(0);
  const [audience, setAudience] = useState<Audience>(start.audience);
  const [people, setPeople] = useState<string[]>([]);
  const [managers, setManagers] = useState<string[]>([me]);
  const members = useResource(loadMembers);
  const available = useResource(loadAvailable);
  const [tools, setTools] = useState<ToolChoices>({});
  const [skills, setSkills] = useState<Skill[]>([]);
  const [pickingSkills, setPickingSkills] = useState(false);
  const mutation = useMutation();
  const first = useRef<HTMLInputElement>(null);
  const ready = [Boolean(draft.name.trim()), Boolean(draft.settings.instructions.trim()), managers.length > 0, true];
  const submit = () => mutation.run(async () => {
    const input = { ...draft, audience, people: audience === 'selected' ? people : [], managers };
    // A saved agent keeps its revision if the picture upload fails, so a retry updates it.
    await saveAgentWithPicture(input, file, next => setDraft(old => ({ ...old, revision: next.revision, settings: next.settings })));
    setFile(undefined);
    for (const connection of available.data?.connections ?? []) {
      const choice = tools[connection.id];
      if (!choice?.on) continue;
      await switchTool(draft.id, connection.id, true);
      for (const operation of connection.operations) {
        const mode = choice.modes[operation.id];
        if (mode && mode !== operation.workspace_mode) await saveRule(draft.id, { connection: connection.id, operation: operation.id, mode, revision: 0 });
      }
    }
    // Assigning is idempotent, so a retry after a failure adds each skill once.
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
      {step === 1 && <InstructionsStep name={draft.name} title={draft.role} text={draft.settings.instructions} rules={directory.system_prompt}
        change={(text, title) => setDraft(old => ({ ...old, role: title ?? old.role, settings: { ...old.settings, instructions: text } }))} />}
      {step === 1 && <section className="agent-create-skills" aria-label="Skills">
        <h3>Skills <small>How-to steps loaded only when needed</small></h3>
        <div className="skill-chips">{skills.map(skill => <span key={skill.id} className="skill-chip">{skill.name}
          <button type="button" aria-label={`Remove ${skill.name}`} onClick={() => setSkills(old => old.filter(s => s.id !== skill.id))}>×</button></span>)}
          <Button onClick={() => setPickingSkills(true)}>＋ Add skills</Button></div>
      </section>}
      {step === 2 && <PeopleStep name={draft.name} workspace={currentWorkspace()?.name ?? 'this workspace'} me={me}
        members={members.data?.users} audience={audience} people={people} managers={managers}
        setAudience={setAudience} setPeople={setPeople} setManagers={setManagers} />}
      {step === 3 && <ToolsStep name={draft.name} connections={available.data?.connections} choices={tools} change={setTools} />}
    </fieldset>
    {pickingSkills && <SkillPicker owner={me} chosen={skills.map(s => s.id)} close={() => setPickingSkills(false)}
      choose={skill => setSkills(old => old.some(s => s.id === skill.id) ? old : [...old, skill])} />}
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
