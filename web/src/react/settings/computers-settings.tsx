import { useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { identityRequest, workspaceId } from '../../workspace-context';
import { ModePicker } from '../agents/mode-picker';
import { InternetPicker, Websites } from '../agents/computer/computer-controls';
import { sizeNames, workingTime, type Internet, type Programs } from '../agents/computer/model';
import { dollars } from '../usage/usage-model';
import '../agents/computer/computer.css';
import '../agents/tools.css';
import '../usage/usage.css';

/** The workspace's limits on agent computers; agents' own settings can only be stricter. */
export type ComputerPolicy = { allowed: boolean; sizes: string[]; internet: Internet; websites: string[]; programs: Programs; browser: Programs; submit: Programs; max_awake: number };
/** One agent's computer this month, for admins. */
type AgentComputer = { agent: string; name: string; size: string; state: 'asleep' | 'awake' | 'deleting'; in_use: boolean; waiting: number; seconds: number; charge_usd: number };
type PolicyView = { policy: ComputerPolicy; can_change: boolean; configured: boolean; computers?: AgentComputer[] };
const path = `/workspaces/${workspaceId}/computer-policy`;
const loadPolicy = () => identityRequest<PolicyView>(path);

/** Settings → Computers (designs/agent-computers 5): whether agents may have computers,
 * which sizes, the loosest internet and the loosest rules. They apply from each
 * computer's next use. */
export function ComputersSettings() {
  const view = useResource(loadPolicy);
  const mutation = useMutation();
  const [edits, setEdits] = useState<ComputerPolicy>();
  const saved = view.data?.policy;
  const draft = edits ?? saved;
  const changed = Boolean(draft && saved && JSON.stringify(draft) !== JSON.stringify(saved));
  const editable = Boolean(view.data?.can_change);
  const save = () => void mutation.run(async () => {
    view.setData(await identityRequest<PolicyView>(path, draft));
    setEdits(undefined);
    mutation.setStatus('Computer limits saved. They apply from each computer’s next use.');
  });
  const set = (next: Partial<ComputerPolicy>) => draft && setEdits({ ...draft, ...next });
  return <section className="settings-computers" aria-labelledby="settings-computers-title">
    <h2 id="settings-computers-title">Computers</h2>
    <p className="ui-help">Agents can have their own Linux computer for files, programs and websites. These limits apply to every agent; an agent’s owner can only make its own settings stricter.</p>
    {view.error && <p role="alert">{view.error}</p>}
    {!draft && !view.error && <p role="status">Loading computer limits…</p>}
    {view.data && !view.data.configured && <p className="ui-help" role="note">Computers aren’t set up on this Cuckoo yet, so no agent can have one. These limits are kept for when they are.</p>}
    {draft && <fieldset className="ui-fieldset" disabled={!editable || mutation.busy}>
      <div className="agent-tool-action">
        <span><strong>Agents may have computers</strong><small>Off stops every agent’s computer from its next use. Files are kept.</small></span>
        <label className="ui-switch"><input type="checkbox" role="switch" checked={draft.allowed} onChange={event => set({ allowed: event.target.checked })} />
          <span>Allow computers</span></label>
      </div>
      {draft.allowed && <>
        <fieldset className="settings-computer-sizes"><legend>Sizes agents may use</legend>
          {['small', 'standard', 'large'].map(size => <label key={size} className="ui-check"><input type="checkbox" checked={draft.sizes.includes(size)}
            onChange={event => set({ sizes: event.target.checked ? ['small', 'standard', 'large'].filter(s => s === size || draft.sizes.includes(s)) : draft.sizes.filter(s => s !== size) })} />
            {sizeNames[size]}</label>)}
        </fieldset>
        {!draft.sizes.length && <p className="ui-help" role="note">With no sizes, no computer can start.</p>}
        <div className="agent-tool-action">
          <span><strong>Internet</strong><small>{draft.internet === 'any' ? 'Agents’ owners choose.' : draft.internet === 'none' ? 'No computer reaches any website.' : 'Computers reach only these websites, or fewer.'}</small></span>
          <InternetPicker value={draft.internet} disabled={!editable || mutation.busy} change={internet => set({ internet })} />
        </div>
        {draft.internet === 'listed' && <Websites sites={draft.websites} limit={100} disabled={!editable || mutation.busy} change={websites => set({ websites })} />}
        <label className="ui-field settings-computer-awake">Computers awake at once
          <input type="number" min={0} max={1000} value={draft.max_awake} onChange={event => set({ max_awake: Math.max(0, Math.min(1000, Number(event.target.value) || 0)) })} />
          <small>{draft.max_awake ? `When ${draft.max_awake} are working, other agents wait their turn.` : '0 means no limit.'}</small></label>
        {([['programs', 'Run programs and commands'], ['browser', 'Open and read websites'], ['submit', 'Press buttons and send forms on websites']] as const).map(([key, label]) =>
          <div key={key} className="agent-tool-action"><span><strong>{label}</strong><small>The loosest rule any agent may have.</small></span>
            <ModePicker label={label} value={draft[key]} disabled={!editable || mutation.busy} change={mode => set({ [key]: mode })} /></div>)}
      </>}
    </fieldset>}
    {editable && draft && <div className="agent-save-row">
      <Button primary disabled={mutation.busy || !changed} onClick={save}>Save computer limits</Button>
      {changed && <Button disabled={mutation.busy} onClick={() => setEdits(undefined)}>Undo changes</Button>}
      <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    </div>}
    {view.data && !editable && <p className="ui-help">Only admins change these.</p>}
    {Boolean(view.data?.computers?.length) && <AgentComputers rows={view.data!.computers!} />}
  </section>;
}

const states = { asleep: 'Asleep', awake: 'Working', deleting: 'Being reset' };

/** Each agent's computer and this month's working time and charge. */
function AgentComputers({ rows }: { rows: AgentComputer[] }) {
  return <div className="usage-table-wrap"><table className="usage-table">
    <caption>Agents’ computers this month</caption>
    <thead><tr><th scope="col">Agent</th><th scope="col">Size</th><th scope="col">Now</th><th scope="col">Working time</th><th scope="col">Charges</th></tr></thead>
    <tbody>{rows.map(row => <tr key={row.agent}>
      <th scope="row">{row.name}</th><td>{sizeNames[row.size] ?? row.size}</td>
      <td>{states[row.state]}{row.in_use ? ', in use' : ''}{row.waiting ? ` · ${row.waiting} waiting` : ''}</td>
      <td>{workingTime(row.seconds)}</td><td>{dollars(row.charge_usd)}</td>
    </tr>)}</tbody>
  </table></div>;
}
