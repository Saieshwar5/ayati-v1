import { AgentSkills } from './skills/agent-skills';
import { useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { saveAgent, type Agent, type Settings } from './model';
import { Knowledge } from './knowledge';
import { AlwaysRead } from '../library/always-read';
import { EverythingItReceives, SystemPromptCard } from './system-prompt';

type Draft = Pick<Settings, 'instructions' | 'memory' | 'history'>;
const pick = (settings: Settings): Draft => ({ instructions: settings.instructions, memory: settings.memory, history: settings.history });

/** Instructions: the owner and admins edit them in place; others read them. The
 * Cuckoo system prompt, what it knows and channel history sit beside them
 * (designs/agents-simplified 8). Skills arrive with M6. */
export function InstructionsTab({ agent, rules, saved }: { agent: Agent; rules: string; saved: () => Promise<void> }) {
  // The draft keeps the revision it started from, so a save never overwrites
  // someone else's newer change.
  const [base, setBase] = useState(agent);
  const [draft, setDraft] = useState<Draft>(() => pick(agent.settings));
  const [previewing, setPreviewing] = useState(false);
  const mutation = useMutation();
  const changed = JSON.stringify(draft) !== JSON.stringify(pick(base.settings));
  if (!changed && base.revision !== agent.revision) { setBase(agent); setDraft(pick(agent.settings)); }
  const save = () => void mutation.run(async () => {
    const next = await saveAgent({ ...base, people: undefined, managers: undefined, settings: { ...base.settings, ...draft } });
    setBase({ ...base, revision: next.revision, settings: { ...base.settings, ...draft } });
    mutation.setStatus('Instructions saved.');
    await saved();
  });
  const edit = (patch: Partial<Draft>) => { mutation.setStatus(''); setDraft(old => ({ ...old, ...patch })); };
  return <div className="agent-instructions-tab">
    <div className="agent-create-main">
      <section aria-label="Instructions">
        {agent.editable
          ? <label className="ui-field">Instructions <small>Character, job and rules, in your own words</small>
            <textarea rows={16} maxLength={24000} value={draft.instructions} onChange={event => edit({ instructions: event.target.value })} /></label>
          : <><h3>Instructions</h3><p className="agent-instructions-text">{agent.settings.instructions || 'No instructions yet.'}</p></>}
        {agent.editable && <div className="agent-save-row">
          <Button primary disabled={mutation.busy || !changed} onClick={save}>Save changes</Button>
          {changed && <Button disabled={mutation.busy} onClick={() => { setDraft(pick(base.settings)); mutation.setStatus(''); }}>Discard</Button>}
          <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
        </div>}
      </section>
      <AgentSkills agent={agent} />
      <section className="agent-knows" aria-label={`What ${agent.name} knows`}>
        <h3>What {agent.name} knows</h3>
        {agent.editable && <label className="ui-check"><input type="checkbox" checked={draft.memory} onChange={event => edit({ memory: event.target.checked })} />Use approved notes in its answers</label>}
        <Knowledge agent={agent} />
        <AlwaysRead agent={agent} />
      </section>
      <details className="agent-advanced"><summary>Advanced</summary>
        <label className="ui-field">Channel history<select disabled={!agent.editable} value={draft.history} onChange={event => edit({ history: event.target.value })}>
          <option value="all">Recent history, including before it joined</option>
          <option value="since_join">Only messages since it joined</option>
        </select></label>
        <p className="ui-help">Changes here are saved with Save changes.</p>
      </details>
    </div>
    <aside className="agent-create-side">
      <SystemPromptCard name={agent.name} rules={rules} open={() => setPreviewing(true)} />
    </aside>
    {previewing && <EverythingItReceives draft={{ name: agent.name, role: agent.role, instructions: draft.instructions, agent: agent.id }} close={() => setPreviewing(false)} />}
  </div>;
}
