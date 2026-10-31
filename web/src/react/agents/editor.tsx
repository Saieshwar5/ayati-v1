import { useRef, useState } from 'react';
import { Button, Dialog, Tabs } from '../ui';
import { useMutation } from '../requests';
import { audienceLabels, base, saveAgent, symbols, type Agent, type Audience, type Directory, type Settings } from './model';
import { AgentFace, defaultRobot } from '../faces';
import { Knowledge } from './knowledge';

export function AgentEditor({ agent, directory, creating, close, saved }: { agent: Agent; directory: Directory; creating: boolean; close: () => void; saved: () => Promise<void> }) {
  // New agents start with a robot picture chosen from their ID.
  const [draft, setDraft] = useState(() => creating && symbols[agent.settings.avatar] ? { ...agent, settings: { ...agent.settings, avatar: defaultRobot(agent.id) } } : agent);
  const [step, setStep] = useState(0);
  const [file, setFile] = useState<File>();
  const mutation = useMutation();
  const first = useRef<HTMLInputElement>(null);
  const editable = agent.editable;
  const change = (key: keyof Agent, value: string) => setDraft(old => ({ ...old, [key]: value }));
  const setting = <K extends keyof Settings>(key: K, value: Settings[K]) => setDraft(old => ({ ...old, settings: { ...old.settings, [key]: value } }));
  async function submit() {
    let next = await saveAgent(draft);
    // Keep the returned revision if an upload fails; retrying never recreates an agent.
    setDraft(old => ({ ...old, revision: next.revision }));
    if (file) {
      const response = await fetch(`/api/identity${base}/${agent.id}/avatar`, { method: 'POST', headers: { 'X-Ayati-Request': 'identity' }, body: file });
      const result = await response.json();
      if (!response.ok) throw new Error(result.error || 'Avatar upload failed. Your profile was saved.');
      next = await saveAgent({ ...next, settings: { ...next.settings, avatar: result.avatar } });
      setDraft(old => ({ ...old, revision: next.revision, settings: next.settings }));
      setFile(undefined);
    }
    await saved();
  }
  const profile = <div className="ui-form">
    <label className="ui-field">Name<input ref={first} required maxLength={80} value={draft.name} onChange={e => change('name', e.target.value)} /></label>
    <label className="ui-field">Job title<input maxLength={80} placeholder="For example, Customer support" value={draft.role} onChange={e => change('role', e.target.value)} /></label>
    <label className="ui-field">Who can talk to it<select disabled={!editable} value={draft.audience} onChange={e => change('audience', e.target.value)}>{(Object.keys(audienceLabels) as Audience[]).map(value => <option key={value} value={value}>{audienceLabels[value]}</option>)}</select></label>
    <p className="ui-help">{draft.audience === 'only_me' ? 'Only you can talk to it, and it cannot join channels. Admins can see its settings.' : 'Anyone in a channel it belongs to can ask it there. Personal conversations are never copied into channels.'}</p>
    <fieldset className="agent-pictures"><legend>Picture</legend>
      <div className="agent-picture-grid">{Array.from({ length: 24 }, (_, index) => `robot-${String(index + 1).padStart(2, '0')}`).map(robot =>
        <button type="button" key={robot} className="agent-picture-choice" aria-label={`Robot picture ${robot.slice(6)}`} aria-pressed={!file && draft.settings.avatar === robot}
          onClick={() => { setting('avatar', robot); setFile(undefined); }}><AgentFace agent={{ id: agent.id, name: draft.name, avatar: robot }} /></button>)}</div>
    </fieldset>
    <label className="ui-field">Or upload an image<input type="file" accept="image/png,image/jpeg,image/webp" onChange={e => { const image = e.target.files?.[0]; if (image && image.size > 2 * 1024 * 1024) { mutation.setStatus('Choose an image smaller than 2 MB.'); e.target.value = ''; return; } setFile(image); }} /></label>
  </div>;
  const capabilities = <div className="ui-form">
    <label className="ui-field">Model provider<select value={draft.settings.provider} onChange={e => setting('provider', e.target.value)}><option value="">Workspace default</option>{directory.providers.map(p => <option key={p.id} value={p.id} disabled={!p.configured}>{p.name}{p.configured ? '' : ' · not connected'}</option>)}</select></label>
    <label className="ui-field">Model identifier<input required maxLength={200} value={draft.model} onChange={e => change('model', e.target.value)} /></label>
    <p className="ui-help">Uses the server’s configured model provider. A model identifier must be supported by that provider.</p>
    <label><input type="checkbox" checked={draft.settings.memory} onChange={e => setting('memory', e.target.checked)} /> Use approved knowledge</label>
    <p className="ui-help">Only approved notes for this agent and the current conversation are included. Turning this off retains saved notes.</p>
    <p className="ui-help">Manage supported business operations in Tools and approvals on the Agents page. Browser access remains unavailable in this conversation runtime. Existing local agents retain their tools.</p>
  </div>;
  const controls = <div className="ui-form">
    <label className="ui-field">Channel context<select value={draft.settings.history} onChange={e => setting('history', e.target.value)}><option value="all">Recent history, including before joining</option><option value="since_join">Only messages since joining</option></select></label>
    <p className="ui-help">Responses use a bounded recent context from the current conversation. An agent responds when mentioned or directly messaged.</p>
    <label><input type="checkbox" checked={draft.settings.paused} onChange={e => setting('paused', e.target.checked)} /> Pause this agent</label>
  </div>;
  const instructions = <div className="ui-form"><label className="ui-field">Instructions<textarea rows={8} maxLength={24000} placeholder="Its character, its job and its rules. For example, when to ask a person." value={draft.settings.instructions} onChange={e => setting('instructions', e.target.value)} /></label><details><summary>Cuckoo rules (always included)</summary><p className="agent-prompt">{directory.system_prompt}</p></details></div>;
  const review = <div className="ui-form"><h3>{draft.name || 'Your agent'}</h3><p>{draft.role}</p>
    <p>{audienceLabels[draft.audience]} · {draft.model}</p><p>Knowledge {draft.settings.memory ? 'enabled' : 'disabled'} · {draft.settings.paused ? 'Paused' : 'Available for messages'}</p>{instructions}</div>;
  const steps = [{ title: 'Identity', content: profile }, { title: 'Capabilities', content: capabilities }, { title: 'Controls', content: controls }, { title: 'Review', content: review }];
  return <Dialog drawer title={creating ? 'Create an agent' : draft.name} close={close} busy={mutation.busy} status={mutation.status} initialFocus={first} footer={editable && <>
    {creating && step > 0 && <Button disabled={mutation.busy} onClick={() => setStep(step - 1)}>Back</Button>}
    {creating && step < 3 ? <Button primary disabled={mutation.busy || !draft.name.trim() || !draft.model.trim()} onClick={() => setStep(step + 1)}>Continue</Button> : <Button primary disabled={mutation.busy || !draft.name.trim() || !draft.settings.instructions.trim() || !draft.model.trim()} onClick={() => void mutation.run(submit)}>{creating ? 'Create agent' : 'Save changes'}</Button>}
  </>}>
    {!creating && <p className="ui-help">Created by {agent.creator_name || 'Unknown'} · Maintained by {agent.owner_name || 'Unknown'}</p>}
    <fieldset className="ui-fieldset" disabled={!editable}>{creating ? <><p className="ui-help">Step {step + 1} of 4 · {steps[step].title}</p>{steps[step].content}</> : <Tabs sections={[...steps.slice(0, 3), { title: 'Instructions', content: instructions }]} />}</fieldset>
    {!creating && <Knowledge agent={agent} />}
  </Dialog>;
}
