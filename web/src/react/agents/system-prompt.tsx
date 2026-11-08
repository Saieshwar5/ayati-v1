import { useEffect, useState } from 'react';
import { Button, Dialog } from '../ui';
import { errorMessage } from '../requests';
import { previewPrompt, type PromptDraft, type PromptPreview, type PromptSection } from './model';
import './system-prompt.css';

const states: Record<PromptSection['state'], { mark: string; label: string }> = {
  included: { mark: '✓', label: 'Included' },
  not_included: { mark: '–', label: 'Not included' },
  conversation: { mark: '', label: 'In some conversations' },
};

/** Everything an agent receives (designs/agent-built-skills 6): the Cuckoo system
 * prompt in sections, each marked included or not and why, then the agent's own
 * part. The server builds it from the same text real requests use. */
export function EverythingItReceives({ draft, close }: { draft: PromptDraft; close: () => void }) {
  const [state, setState] = useState<{ preview?: PromptPreview; error?: string }>({});
  const [status, setStatus] = useState('');
  const { name, role, instructions, agent, apps } = draft;
  const skills = JSON.stringify(draft.skills ?? []);
  useEffect(() => {
    const controller = new AbortController();
    setState({});
    previewPrompt({ name, role, instructions, agent, apps, skills: JSON.parse(skills) }, controller.signal)
      .then(preview => { if (!controller.signal.aborted) setState({ preview }); })
      .catch(error => { if (!controller.signal.aborted) setState({ error: errorMessage(error) }); });
    return () => controller.abort();
  }, [name, role, instructions, agent, apps, skills]);
  const display = name.trim() || 'Your agent';
  const preview = state.preview;
  const copy = () => {
    if (!preview) return;
    const unavailable = () => setStatus('Copy isn’t available here. Open each section to select its text.');
    const writing = navigator.clipboard?.writeText(preview.prompt);
    if (!writing) { unavailable(); return; }
    writing.then(() => setStatus('Copied the full text.'), unavailable);
  };
  return <Dialog drawer title={`Everything ${display} receives`} close={close} status={status}
    footer={<Button disabled={!preview} onClick={copy}>Copy as text</Button>}>
    {!preview && !state.error && <p role="status">Loading…</p>}
    {state.error && <p role="alert">{state.error}</p>}
    {preview && <div className="system-prompt">
      <p className="ui-help">In this order, with every message. Built from the same text as a real request.</p>
      <section aria-labelledby="system-prompt-cuckoo">
        <h3 id="system-prompt-cuckoo" className="system-prompt-layer">1 · Cuckoo system prompt
          <small>Same for every agent · {preview.version}</small></h3>
        <p className="ui-help">Set by Cuckoo. Nobody in the workspace can change it. Parts are added only for what {display} can use.</p>
        <ul className="system-prompt-sections">{preview.sections.map(section => <SectionRow key={section.id} section={section} />)}</ul>
      </section>
      <section aria-labelledby="system-prompt-own">
        <h3 id="system-prompt-own" className="system-prompt-layer">2 · {display}’s instructions</h3>
        {preview.instructions.trim()
          ? <pre className="system-prompt-text">{preview.instructions.trim()}</pre>
          : <p className="ui-help">No instructions yet.</p>}
      </section>
      <section aria-labelledby="system-prompt-skills">
        <h3 id="system-prompt-skills" className="system-prompt-layer">3 · Skill list</h3>
        {preview.skills.length
          ? <ul className="system-prompt-skills">{preview.skills.map(skill => <li key={skill.name}><strong>{skill.name}</strong>: {skill.description}</li>)}</ul>
          : <p className="ui-help">No skills yet.</p>}
        <p className="ui-help">Only each skill’s name and description are sent every time.</p>
      </section>
      <section aria-labelledby="system-prompt-each">
        <h3 id="system-prompt-each" className="system-prompt-layer">4 · Added for each message</h3>
        <p className="ui-help">Who is talking and where, recent messages, and {display}’s tools with their rules.</p>
      </section>
      <section aria-labelledby="system-prompt-needed">
        <h3 id="system-prompt-needed" className="system-prompt-layer">5 · Only when needed</h3>
        <p className="ui-help">A skill’s steps and files, when {display} opens it. Activity shows each skill it used.</p>
      </section>
    </div>}
  </Dialog>;
}

function SectionRow({ section }: { section: PromptSection }) {
  const state = states[section.state];
  return <li className={`system-prompt-section system-prompt-${section.state}`}>
    <details>
      <summary>
        <span className="system-prompt-mark" aria-hidden="true">{state.mark}</span>
        <span className="system-prompt-title">{section.title}</span>
        <span className="system-prompt-reason">{state.label} · {section.reason}</span>
      </summary>
      <pre className="system-prompt-text">{section.text.trim()}</pre>
    </details>
  </li>;
}

/** The locked system prompt beside an agent's instructions, with a way to read all of it. */
export function SystemPromptCard({ name, rules, open }: { name: string; rules: string; open: () => void }) {
  return <section className="agent-rules" aria-label="Cuckoo system prompt">
    <h3>Cuckoo system prompt <small>Always included</small></h3>
    <p>{rules}</p>
    <p className="ui-help">The same for every agent; nobody in the workspace can change it. This is its first part: more is added for what {name} can use, and for who it is talking to.</p>
    <Button className="ui-link" onClick={open}>See everything {name} receives</Button>
  </section>;
}
