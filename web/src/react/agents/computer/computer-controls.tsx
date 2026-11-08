import { useState } from 'react';
import { Button } from '../../ui';
import { useMutation } from '../../requests';
import { saveAgent, type Agent } from '../model';
import { ModePicker } from '../mode-picker';
import { computerOf, internetLabels, website, type ComputerSettings, type Internet } from './model';
import './computer.css';

const internets: Internet[] = ['none', 'listed', 'any'];

/** What the agent may do on its computer (designs/agent-computers 8): its internet,
 * enforced on the computer's network, and whether it runs programs. Files always
 * work. Used on the Tools tab and in the create flow's Controls step. */
export function ComputerControls({ name, value, change, disabled }: {
  name: string; value: ComputerSettings; change: (next: ComputerSettings) => void; disabled: boolean;
}) {
  const [adding, setAdding] = useState('');
  const added = website(adding);
  const add = () => {
    if (!added || value.websites.includes(added)) return;
    change({ ...value, websites: [...value.websites, added] });
    setAdding('');
  };
  return <div className="agent-computer-controls">
    <div className="agent-tool-action">
      <span><strong id={`internet-${name}`}>Internet</strong>
        <small>{value.internet === 'none' ? `${name}’s computer can’t reach any website.`
          : value.internet === 'any' ? 'Programs on the computer can reach any website.'
          : 'Programs on the computer can reach only these websites.'}</small></span>
      <InternetPicker value={value.internet} disabled={disabled} change={internet => change({ ...value, internet })} />
    </div>
    {value.internet === 'listed' && <div className="agent-computer-sites">
      <ul aria-label="Allowed websites">{value.websites.map(site => <li key={site}>{site}
        <button type="button" aria-label={`Remove ${site}`} disabled={disabled}
          onClick={() => change({ ...value, websites: value.websites.filter(s => s !== site) })}>×</button></li>)}</ul>
      {!value.websites.length && <p className="ui-help">No websites yet, so it reaches none.</p>}
      <label className="ui-field">Add a website<input value={adding} maxLength={253} disabled={disabled || value.websites.length >= 30}
        placeholder="portal.example.in or *.example.in" onChange={event => setAdding(event.target.value)}
        onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); add(); } }} /></label>
      <Button disabled={disabled || !added || value.websites.includes(added)} onClick={add}>Add</Button>
      {adding.trim() && !added && <p className="ui-help" role="note">Enter a host name such as portal.example.in.</p>}
    </div>}
    <div className="agent-tool-action">
      <span><strong>Run programs and commands</strong>
        <small>{value.programs === 'ask' ? 'Each new command asks the person who asked first.'
          : value.programs === 'block' ? `${name} can only save and read files.` : `${name} runs commands in each request’s own folder.`}</small></span>
      <ModePicker label="Run programs and commands" value={value.programs} disabled={disabled} change={next => change({ ...value, programs: next })} />
    </div>
    <div className="agent-tool-action">
      <span><strong>Open and read websites (browser)</strong>
        <small>{value.browser === 'ask' ? 'Each new website asks the person who asked first.'
          : value.browser === 'block' ? `${name} has no browser.` : `${name} reads websites its Internet setting allows.`}</small></span>
      <ModePicker label="Open and read websites" value={value.browser} disabled={disabled} change={next => change({ ...value, browser: next })} />
    </div>
    {value.browser !== 'block' && <div className="agent-tool-action">
      <span><strong>Press buttons and send forms on websites</strong>
        <small>{value.submit === 'ask' ? `${name} asks before pressing a button, sending a form or pressing Enter.`
          : value.submit === 'block' ? `${name} only reads, follows links and fills in fields.` : `${name} presses buttons and sends forms without asking.`}
          {' '}Passwords and one-time codes are always typed by a person.</small></span>
      <ModePicker label="Press buttons and send forms" value={value.submit} disabled={disabled} change={next => change({ ...value, submit: next })} />
    </div>}
    <div className="agent-tool-action">
      <span><strong>Save, read and change files on the computer</strong><small>Always allowed while the computer is on.</small></span>
    </div>
  </div>;
}

/** Internet in the same Allow / Ask first / Block style as other controls. */
function InternetPicker({ value, disabled, change }: { value: Internet; disabled: boolean; change: (next: Internet) => void }) {
  return <div className="mode-picker" role="radiogroup" aria-label="Internet">
    {internets.map(option => <label key={option} className={`mode-option${value === option ? ' chosen' : ''}`}>
      <input type="radio" name="computer-internet" value={option} checked={value === option} disabled={disabled} onChange={() => change(option)} />
      {internetLabels[option]}
    </label>)}
  </div>;
}

/** Tools tab: the computer's controls, saved with the agent by its owner or an admin. */
export function ComputerToolsCard({ agent, saved }: { agent: Agent; saved: () => Promise<void> }) {
  const mutation = useMutation();
  const settings = computerOf(agent.settings);
  const editable = Boolean(agent.editable);
  // Only edits are kept here; without any, the saved controls show.
  const [edits, setEdits] = useState<ComputerSettings>();
  const draft = edits ?? settings;
  const changed = JSON.stringify(draft) !== JSON.stringify(settings);
  const save = () => void mutation.run(async () => {
    await saveAgent({ ...agent, people: undefined, managers: undefined, settings: { ...agent.settings, computer: draft } });
    await saved();
    setEdits(undefined);
    mutation.setStatus('Computer controls saved. They apply from its next command.');
  });
  if (!settings.on) return null;
  return <article className="agent-tool" aria-labelledby="agent-computer-tools-title">
    <header><span><strong id="agent-computer-tools-title">{agent.name}’s computer</strong>
      <small>Cuckoo checks these before every command, and the computer’s network allows only what Internet allows.</small></span></header>
    <ComputerControls name={agent.name} value={draft} change={setEdits} disabled={!editable || mutation.busy} />
    {editable && <div className="agent-save-row">
      <Button primary disabled={mutation.busy || !changed} onClick={save}>Save computer controls</Button>
      {changed && <Button disabled={mutation.busy} onClick={() => setEdits(undefined)}>Undo changes</Button>}
      <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    </div>}
    {!editable && <p className="ui-help">Only {agent.name}’s owner and admins can change these.</p>}
  </article>;
}
