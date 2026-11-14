import { useCallback, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { saveAgent, type Agent } from '../model';
import { computerOf, loadComputer, perHour, resetComputer, sizeFacts, sizeNames, sizeUses, workingTime, type ComputerOffer, type ComputerSettings, type ComputerStatus } from './model';
import { dollars } from '../../usage/usage-model';
import { LiveView } from './live-view';
import './computer.css';

/** Environment → Computer (designs/agent-computers 1): on or off, its state in plain
 * words, its size with the hourly price while working, this month's use and Reset.
 * The provider is never named. Only the agent's owner and admins change it. */
export function ComputerCard({ agent, offer, saved }: { agent: Agent; offer: ComputerOffer; saved: () => Promise<void> }) {
  const load = useCallback(() => loadComputer(agent.id), [agent.id]);
  const status = useResource(load);
  const mutation = useMutation();
  const [confirming, setConfirming] = useState(false);
  const [watching, setWatching] = useState(false);
  const settings = computerOf(agent.settings);
  const editable = Boolean(agent.editable);
  const save = (next: ComputerSettings, done: string) => void mutation.run(async () => {
    await saveAgent({ ...agent, people: undefined, managers: undefined, settings: { ...agent.settings, computer: next } });
    await saved();
    await status.refresh().catch(() => {});
    mutation.setStatus(done);
  });
  const reset = () => void mutation.run(async () => {
    status.setData(await resetComputer(agent.id));
    setConfirming(false);
    mutation.setStatus(`${agent.name}’s computer is being reset. Its next task starts on a new one.`);
  });
  const computer = status.data?.computer;
  const policy = status.data?.policy;
  return <section className="agent-computer" aria-labelledby="agent-computer-title">
    <header className="agent-computer-head">
      <span><strong id="agent-computer-title">Computer</strong>
        <small>{agent.name}’s own Linux computer for files and programs</small></span>
      <label className="ui-switch"><input type="checkbox" role="switch" checked={settings.on} disabled={!editable || mutation.busy}
        onChange={event => save({ ...settings, on: event.target.checked }, event.target.checked
          ? `${agent.name} can use its computer. It is made the first time ${agent.name} needs it.`
          : `${agent.name} no longer uses its computer. Its files are kept.`)} />
        <span>Use a computer</span></label>
    </header>
    {policy && !policy.allowed && <p className="ui-help" role="note">An admin switched off computers in this workspace (Settings → Computers), so {agent.name} can’t use one now.</p>}
    {status.data?.workspace_computer && <p className="ui-help" role="note">{agent.name} works on {status.data.workspace_computer}, your workspace’s own computer
      (Settings → Computers). Its commands and files go there, under the Programs rule in its Tools tab; the browser isn’t available there yet.</p>}
    {!settings.on && <p className="ui-help">Off: {agent.name} has no computer tools. When it is on, {agent.name} can read and convert files,
      work through data and run programs on a computer that keeps its files between tasks.</p>}
    {settings.on && <>
      <StateLine name={agent.name} status={status.data} error={status.error} />
      <fieldset className="agent-computer-sizes" disabled={!editable || mutation.busy}>
        <legend>Size</legend>
        {offer.sizes.map(size => <label key={size.size} className={`agent-computer-size${settings.size === size.size ? ' chosen' : ''}`}>
          <input type="radio" name={`computer-size-${agent.id}`} value={size.size} checked={settings.size === size.size}
            disabled={Boolean(policy && !policy.sizes.includes(size.size))}
            onChange={() => save({ ...settings, size: size.size }, `${agent.name}’s computer size is ${sizeNames[size.size] ?? size.size}.`)} />
          <strong>{sizeNames[size.size] ?? size.size}</strong>
          <small>{sizeFacts(size)}</small>
          <small>{sizeUses[size.size]}</small>
          <span>{policy && !policy.sizes.includes(size.size) ? 'Not allowed in this workspace' : perHour(size.usd_per_hour)}</span>
        </label>)}
      </fieldset>
      {computer && computer.size !== settings.size && <p className="ui-help" role="note">
        {agent.name}’s computer is {sizeNames[computer.size] ?? computer.size}. The new size applies after Reset computer; reset wipes its files.</p>}
      <p className="agent-computer-comes">Comes with <span>Files</span><span>Programs</span><span>Browser</span><span>Live view</span>
        {editable && computer?.state === 'awake' && <Button onClick={() => setWatching(true)}>Watch live</Button>}</p>
      <p className="agent-computer-shared">Everyone who can talk to {agent.name} shares this computer. Each request gets its own folder, and one request uses it at a time.</p>
    </>}
    {status.data && (settings.on || computer) && <footer className="agent-computer-month">
      <span>This month <strong>{workingTime(status.data.month.seconds)} working</strong> · {dollars(status.data.month.charge_usd)}</span>
      {status.data.sleep_after_minutes && <span>Sleeps {status.data.sleep_after_minutes} minutes after {agent.name} stops</span>}
      {editable && computer && computer.state !== 'deleting' && <Button className="agent-computer-reset" disabled={mutation.busy} onClick={() => setConfirming(true)}>Reset computer</Button>}
    </footer>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    {!editable && <p className="ui-help">Only {agent.name}’s owner and admins can change its computer.</p>}
    {watching && <LiveView agent={agent.id} name={agent.name} close={() => setWatching(false)} />}
    {confirming && <Dialog title={`Reset ${agent.name}’s computer?`} close={() => setConfirming(false)} busy={mutation.busy} status={mutation.status}
      footer={<><Button disabled={mutation.busy} onClick={() => setConfirming(false)}>Keep it</Button>
        <Button primary disabled={mutation.busy} onClick={reset}>Reset computer</Button></>}>
      <p>Everything on it is deleted: files in every request’s folder and in shared/, and programs it installed. {agent.name} starts on a new computer next time it needs one.</p>
    </Dialog>}
  </section>;
}

/** The computer's state in plain words. Asleep costs nothing. */
function StateLine({ name, status, error }: { name: string; status?: ComputerStatus; error: string }) {
  if (error) return <p className="agent-computer-state" role="alert">{error}</p>;
  if (!status) return <p className="agent-computer-state" role="status">Checking the computer…</p>;
  const computer = status.computer;
  const [label, detail, tone] = !computer
    ? ['Not made yet', `It is made the first time ${name} needs it. Nothing is charged until then.`, 'asleep']
    : computer.state === 'deleting' ? ['Being reset', 'Its files are being deleted. A new computer starts on next use.', 'waiting']
    : computer.state === 'awake' ? ['Working', computer.in_use ? `${name} is using it now.` : `Going to sleep soon.`, 'working']
    : ['Asleep · costs nothing while asleep', `Wakes by itself in a few seconds when ${name} needs it.${computer.last_used ? ` Last worked ${new Date(computer.last_used * 1000).toLocaleString()}.` : ''}`, 'asleep'];
  return <div className={`agent-computer-state ${tone}`}>
    <span aria-hidden="true" className="agent-computer-dot" />
    <span><strong>{label}</strong><small>{detail}</small>
      {computer?.problem && <small className="agent-computer-problem">Last problem: it couldn’t be started. Cuckoo tries again on the next task.</small>}</span>
  </div>;
}
