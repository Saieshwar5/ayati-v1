import { useEffect, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { errorMessage } from '../../requests';
import { previewPrompt } from '../model';
import { starters, untouched } from './starters';

/** Create step 2: one instructions box with starters, the locked Cuckoo rules and
 * "See everything it receives" (design 2-create-instructions). */
export function InstructionsStep({ name, title, text, rules, change }: {
  name: string; title: string; text: string; rules: string; change: (text: string, title?: string) => void;
}) {
  const [note, setNote] = useState('');
  const [previewing, setPreviewing] = useState(false);
  const display = name.trim() || 'this agent';
  const start = (id: string) => {
    const starter = starters.find(item => item.id === id)!;
    if (!untouched(text, name)) { setNote('Starters fill an empty box. Clear the box first to use one.'); return; }
    setNote('');
    change(starter.text(name), title.trim() ? undefined : starter.title);
  };
  return <div className="agent-create-instructions">
    <div className="agent-create-main">
      <div className="agent-starters" role="group" aria-label="Start from">
        <span>Start from</span>
        {starters.map(starter => <Button key={starter.id} onClick={() => start(starter.id)}>{starter.label}</Button>)}
      </div>
      {note && <p className="ui-help" role="status">{note}</p>}
      <label className="ui-field">Instructions <small>Character, job and rules, in your own words</small>
        <textarea rows={14} maxLength={24000} value={text} placeholder={`What ${display} does, how it talks, what it must never do and when to ask a person.`}
          onChange={event => change(event.target.value)} />
      </label>
    </div>
    <aside className="agent-create-side">
      <section className="agent-rules" aria-label="Cuckoo rules">
        <h3>Cuckoo rules <small>Always included</small></h3>
        <p>{rules}</p>
        <p className="ui-help">Set by Cuckoo. Nobody in the workspace can change these.</p>
      </section>
      <section className="agent-rules" aria-label="Added automatically">
        <h3>Added automatically</h3>
        <ul><li>Its name and job title</li><li>Who it is talking to, and where</li><li>Its tools and their rules</li></ul>
        <Button className="ui-link" onClick={() => setPreviewing(true)}>See everything {display} receives</Button>
      </section>
    </aside>
    {previewing && <PromptPreview draft={{ name: name.trim() || 'Your agent', role: title, instructions: text }} close={() => setPreviewing(false)} />}
  </div>;
}

/** The exact text the server would give an agent with this draft. */
function PromptPreview({ draft, close }: { draft: { name: string; role: string; instructions: string }; close: () => void }) {
  const [state, setState] = useState<{ prompt?: string; error?: string }>({});
  const { name, role, instructions } = draft;
  useEffect(() => {
    const controller = new AbortController();
    previewPrompt({ name, role, instructions }, controller.signal)
      .then(result => { if (!controller.signal.aborted) setState({ prompt: result.prompt }); })
      .catch(error => { if (!controller.signal.aborted) setState({ error: errorMessage(error) }); });
    return () => controller.abort();
  }, [name, role, instructions]);
  return <Dialog title={`Everything ${name} receives`} close={close}>
    {!state.prompt && !state.error && <p role="status">Loading…</p>}
    {state.error && <p role="alert">{state.error}</p>}
    {state.prompt && <pre className="agent-prompt">{state.prompt}</pre>}
    <p className="ui-help">Messages, the people it talks to and its tools are added for each request.</p>
  </Dialog>;
}
