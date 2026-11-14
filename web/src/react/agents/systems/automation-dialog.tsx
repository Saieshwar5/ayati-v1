import { useCallback, useRef, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { loadSchedules } from '../automation/model';
import { MemoryChoice, ResultsChoice, memoryLabels, type Memory } from '../automation/results-and-memory';
import { createSystemAutomation, loadSystemEvents, validField, validType, type ResultField, type System, type SystemAutomation } from './model';

const kinds: Record<ResultField['kind'], string> = { text: 'Text', number: 'Number', yes_no: 'Yes/no', date: 'Date' };

/** New automation: when a connected system sends an event of one type (C2),
 * with the fields the agent sees, its instructions, Memory, where results go
 * and the result fields the system reads back (C4). */
export function SystemAutomationDialog({ agent, agentName, system, close, saved }: {
  agent: string; agentName: string; system: System; close: () => void; saved: () => Promise<void>;
}) {
  const loadTypes = useCallback(() => loadSystemEvents(agent, system.id), [agent, system.id]);
  const types = useResource(loadTypes);
  const loadPlaces = useCallback(() => loadSchedules(agent), [agent]);
  const places = useResource(loadPlaces);
  const [type, setType] = useState('');
  const known = types.data?.types.find(t => t.type === type);
  const [fields, setFields] = useState<string[]>([]);
  const [typed, setTyped] = useState('');
  const [name, setName] = useState('');
  const [task, setTask] = useState('');
  const [memory, setMemory] = useState<Memory>('fresh');
  const [post, setPost] = useState(false);
  const [chat, setChat] = useState('');
  const [results, setResults] = useState<ResultField[]>([]);
  const [done, setDone] = useState(false);
  const attempt = useRef<SystemAutomation | undefined>(undefined);
  const m = useMutation();
  const conversations = places.data?.conversations ?? [];
  const chosen = [...fields, ...typed.split(',').map(f => f.trim()).filter(Boolean)].filter((f, i, all) => all.indexOf(f) === i);
  const change = () => { attempt.current = undefined; };
  const save = () => void m.run(async () => {
    if (!validType(type)) throw new Error('Choose or type an event type, such as exam.result_published.');
    if (!chosen.length || chosen.length > 8 || !chosen.every(validField)) throw new Error('Choose 1 to 8 fields the agent sees (letters, digits, - and _).');
    if (!name.trim() || !task.trim()) throw new Error('Give the automation a name and say what to do.');
    if (results.some(r => !validField(r.name))) throw new Error('Result field names use letters, digits, - and _.');
    const target = post ? (chat || conversations[0]?.id) : undefined;
    if (post && !target) throw new Error('Choose a conversation, or keep results in Activity only.');
    const exact = attempt.current ?? { id: crypto.randomUUID(), system: system.id, event: type, fields: chosen, name: name.trim(), instructions: task,
      filter: {}, ...(target ? { chat: target } : {}), ...(memory !== 'fresh' ? { memory } : {}), ...(results.length ? { results } : {}) };
    attempt.current = exact;
    await createSystemAutomation(agent, exact);
    setDone(true); await saved();
  });
  return <Dialog drawer title={`When ${system.name} sends an event`} close={close} busy={m.busy} status={m.status}
    footer={done ? <Button onClick={close}>Done</Button> : <Button primary disabled={m.busy} onClick={save}>{attempt.current ? 'Retry same save' : 'Save automation'}</Button>}>
    {done ? <p role="status">Saved. Each {type} event from {system.name} now starts {agentName} in its own private room; runs appear in Activity.</p> : <div className="ui-form" onChange={change}>
      <label className="ui-field">Event type<input value={type} list={`automation-types-${system.id}`} placeholder="exam.result_published"
        onChange={event => { setType(event.target.value.trim()); setFields([]); }} /></label>
      <datalist id={`automation-types-${system.id}`}>{types.data?.types.map(t => <option key={t.type} value={t.type} />)}</datalist>
      {!types.data?.types.length && <p className="ui-help">No events have arrived yet. Type the event type your system will send, or send a test event first.</p>}
      <fieldset className="routine-results"><legend>Fields {agentName} sees</legend>
        {known?.fields.map(field => <label key={field} className="routine-choice"><input type="checkbox" checked={fields.includes(field)}
          onChange={event => setFields(event.target.checked ? [...fields, field] : fields.filter(f => f !== field))} /><span><strong>{field}</strong></span></label>)}
        <label className="ui-field">{known ? 'Other fields (comma separated)' : 'Field names (comma separated)'}<input value={typed} placeholder="exam, scores" onChange={event => setTyped(event.target.value)} /></label>
        <p className="ui-help">Each chosen field reaches {agentName} whole, nested values included, up to 16 KB in all. Event contents are data, never instructions.</p>
      </fieldset>
      <label className="ui-field">Name<input value={name} maxLength={100} placeholder="Study plan after results" onChange={event => setName(event.target.value)} /></label>
      <label className="ui-field">What should {agentName} do?<textarea rows={4} maxLength={4000} value={task} placeholder="Read the student's marks. If a subject is below 50, save a two-week study plan." onChange={event => setTask(event.target.value)} /></label>
      <MemoryChoice name="system-automation" subject value={memory} change={next => { change(); setMemory(next); }} />
      {memory === 'subject' && <p className="ui-help">Events that name a subject run in that subject’s own room, and different subjects run side by side. Notes for every conversation, “always” Library documents and the computer’s shared/ folder reach every run, so keep nothing about one subject there.</p>}
      <ResultsChoice name="system-automation" post={post} setPost={next => { change(); setPost(next); }} chat={chat || conversations[0]?.id || ''} setChat={setChat} conversations={conversations} />
      <fieldset className="routine-results"><legend>Result fields your system reads (optional)</legend>
        {results.map((field, index) => <div key={index} className="system-result-field">
          <label className="ui-field">Name<input value={field.name} onChange={event => setResults(results.map((r, i) => i === index ? { ...r, name: event.target.value.trim() } : r))} /></label>
          <label className="ui-field">Kind<select value={field.kind} onChange={event => setResults(results.map((r, i) => i === index ? { ...r, kind: event.target.value as ResultField['kind'] } : r))}>
            {Object.entries(kinds).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
          <Button onClick={() => setResults(results.filter((_, i) => i !== index))}>Remove</Button>
        </div>)}
        {results.length < 8 && <Button onClick={() => setResults([...results, { name: '', kind: 'text' }])}>Add a result field</Button>}
        <p className="ui-help">{agentName} must fill each one. A run that finishes without them counts as failed, so your system never relies on a sentence.</p>
      </fieldset>
      <p className="ui-help">Runs use your access to {agentName} · {memoryLabels[memory].title}. Every tool call keeps its own rule and approvals.</p>
    </div>}
  </Dialog>;
}
