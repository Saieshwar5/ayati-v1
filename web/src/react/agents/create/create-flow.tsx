import { useRef, useState } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { currentWorkspace, workspaceSession } from '../../../workspace-context';
import { defaultRobot } from '../../faces';
import { defaults, saveAgentWithPicture, type Agent, type Audience, type Directory } from '../model';
import { NameStep } from './name-step';
import { SkillsStep } from './skills-step';
import { loadMembers, PeopleStep } from './people-step';
import { ConnectorsStep } from './connectors-step';
import { ControlsStep, type Suggested, type ToolChoices } from './controls-step';
import { CreateModelChoice } from './model-choice';
import { loadAvailable, saveRule, switchTool } from '../control-model';
import { assignSkill, createSkill } from '../skills/model';
import { useSkillWriting } from '../skills/setup-chat';
import { skillText, type DraftSkill, type LibraryPick } from '../skills/writer-model';
import { SkillPicker } from '../skills/skill-picker';
import { SkillIcon } from '../skills/skill-icons';
import { BuiltInTools, searchesWeb, WebSearchChoice } from '../web-search';
import { ComputerChoice } from '../computer/computer-choice';
import { computerOf, type ComputerOffer } from '../computer/model';
import { WhatsappChoice } from '../whatsapp/whatsapp-choice';
import { activateWhatsapp } from '../whatsapp/model';
import { TelegramChoice } from '../telegram/telegram-choice';
import { activateTelegram, loadWorkspaceTelegram, type WorkspaceTelegram } from '../telegram/model';
import './create.css';

const steps = ['Name and picture', 'Tools', 'Skills', 'People', 'Controls'];
const hints = [
  'Only the name is needed. Everything else can change later.',
  'Skipping is fine. Tools can change later on the Tools tab.',
  'Ground rules and skills can be changed at any time.',
  'Defaults are fine for most agents.',
  'Cuckoo checks these before every action. Change them later on the Tools tab.',
];

/** A fresh agent draft owned by the person creating it. */
export function newAgent(directory: Directory): Agent {
  const id = crypto.randomUUID();
  // The workspace's default model, with its provider when the admin listed models.
  // A catalog model never names a provider: Cuckoo chooses who serves it.
  const provider = directory.catalog?.catalog_on ? ''
    : directory.models?.find(choice => choice.model === directory.default_model)?.provider ?? defaults.provider;
  return { id, name: '', role: '', model: directory.default_model, audience: 'everyone', revision: 0,
    settings: { ...defaults, provider, avatar: defaultRobot(id) }, editable: true };
}

const noTelegram = async (): Promise<WorkspaceTelegram | null> => null;

/** Create an agent in five short steps (designs/agent-built-skills): name, the
 * tools and apps it can use, its skills, people, then controls. Tools are saved
 * right after the agent; retrying after a failure updates the same agent and its
 * tools. Web search and a computer are offered when this Cuckoo has them
 * (designs/web-search, designs/agent-computers 7 and 8). The last step also shows
 * the model, the workspace default unless changed there. */
export function AgentCreate({ start, directory, close, created, webSearch = false, computers, whatsappOffered = false, telegramOffered = false }: {
  start: Agent; directory: Directory; close: () => void; created: (id: string) => Promise<void>; webSearch?: boolean; computers?: ComputerOffer | null; whatsappOffered?: boolean; telegramOffered?: boolean;
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
  const [whatsapp, setWhatsapp] = useState(false);
  const [telegram, setTelegram] = useState(false);
  const telegramWorkspace = useResource(telegramOffered ? loadWorkspaceTelegram : noTelegram);
  const [suggested, setSuggested] = useState<Suggested>();
  const [skills, setSkills] = useState<LibraryPick[]>([]);
  const [drafts, setDrafts] = useState<DraftSkill[]>([]);
  const writing = useSkillWriting();
  const chosen = Object.entries(tools).filter(([, choice]) => choice.on).map(([id]) => id);
  const addSkills = (picks: LibraryPick[]) => setSkills(old => [...old, ...picks.filter(pick => !old.some(skill => skill.id === pick.id))]);
  const [pickingSkills, setPickingSkills] = useState(false);
  const mutation = useMutation();
  const first = useRef<HTMLInputElement>(null);
  const ready = [Boolean(draft.name.trim()), true, Boolean(draft.settings.instructions.trim()), managers.length > 0, true];
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
    // Written skills become the owner's own skills. Saving and assigning are
    // idempotent, so a retry after a failure saves and adds each one once.
    for (const written of drafts) {
      const saved = await createSkill(written.id, false, [{ path: 'SKILL.md', text: skillText(written.name, written.description, written.steps) }]);
      await assignSkill(draft.id, saved.id, true);
    }
    for (const skill of skills) await assignSkill(draft.id, skill.id, true);
    // Gives the saved agent its number; repeating it after a failure keeps the same one.
    if (whatsapp) await activateWhatsapp(draft.id);
    // Asks for the agent's own bot; repeating it after a failure keeps one request.
    if (telegram) await activateTelegram(draft.id);
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
      {step === 1 && <ConnectorsStep name={draft.name} connections={available.data?.connections} choices={tools} change={setTools}
        refresh={() => void available.refresh().catch(() => {})}
        builtIn={(webSearch || computers || whatsappOffered || telegramOffered) && <BuiltInTools>
          {webSearch && <WebSearchChoice name={draft.name.trim() || 'This agent'} on={searchesWeb(draft.settings)}
            change={on => setDraft(old => ({ ...old, settings: { ...old.settings, web_search: on } }))} />}
          {computers && <ComputerChoice name={draft.name.trim() || 'This agent'} offer={computers} value={draft.settings.computer}
            change={computer => setDraft(old => ({ ...old, settings: { ...old.settings, computer } }))} />}
          {telegramOffered && <TelegramChoice name={draft.name.trim() || 'This agent'} on={telegram}
            connected={Boolean(telegramWorkspace.data?.connected)} change={setTelegram} />}
          {whatsappOffered && <WhatsappChoice name={draft.name.trim() || 'This agent'} on={whatsapp} change={setWhatsapp} />}
        </BuiltInTools>} />}
      {step === 2 && <SkillsStep name={draft.name} title={draft.role} text={draft.settings.instructions}
        apps={chosen.length > 0} connections={chosen} writing={writing} drafts={drafts} setDrafts={setDrafts} library={skills} reuse={addSkills}
        change={(text, title) => setDraft(old => ({ ...old, role: title ?? old.role, settings: { ...old.settings, instructions: text } }))}>
        <div className="skill-chips">{skills.map(skill => <span key={skill.id} className="skill-chip"><SkillIcon name="book" />{skill.name}
          <button type="button" aria-label={`Remove ${skill.name}`} onClick={() => setSkills(old => old.filter(s => s.id !== skill.id))}>×</button></span>)}
          <Button onClick={() => setPickingSkills(true)}>＋ Add skills</Button></div>
      </SkillsStep>}
      {step === 3 && <PeopleStep name={draft.name} workspace={currentWorkspace()?.name ?? 'this workspace'} me={me}
        members={members.data?.users} audience={audience} people={people} managers={managers}
        setAudience={setAudience} setPeople={setPeople} setManagers={setManagers} />}
      {step === 4 && <ControlsStep name={draft.name} connections={available.data?.connections} choices={tools} change={setTools}
        computer={computers && computerOf(draft.settings).on ? computerOf(draft.settings) : undefined}
        changeComputer={computer => setDraft(old => ({ ...old, settings: { ...old.settings, computer } }))}
        suggested={suggested} setSuggested={setSuggested} request={{ name: draft.name.trim(), role: draft.role, audience, ground_rules: draft.settings.instructions,
          skills: [...drafts, ...skills.map(skill => ({ ...skill, steps: '' }))].map(({ name, description, steps }) => ({ name, description, steps })) }} />}
      {step === 4 && <CreateModelChoice agent={draft} directory={directory}
        needsTools={chosen.length > 0 || (webSearch && searchesWeb(draft.settings)) || Boolean(computers && computerOf(draft.settings).on)}
        change={(model, provider) => setDraft(old => ({ ...old, model, settings: { ...old.settings, provider } }))} />}
    </fieldset>
    {pickingSkills && <SkillPicker agent={draft.name} owner={me} chosen={skills.map(s => s.id)} close={() => setPickingSkills(false)}
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
