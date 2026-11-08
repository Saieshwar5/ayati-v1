import { computerDefaults, perHour, sizeFacts, sizeNames, type ComputerOffer, type ComputerSettings } from './model';
import './computer.css';

/** Create step 2 (designs/agent-computers 7): the computer sits with the other tools,
 * off by default. Switching it on grants nothing yet: what the agent may do on it is
 * chosen in Controls. Nothing is made or charged until the agent first uses it. */
export function ComputerChoice({ name, offer, value, change }: {
  name: string; offer: ComputerOffer; value?: ComputerSettings; change: (next: ComputerSettings) => void;
}) {
  const settings = { ...computerDefaults, ...value };
  const sizes = offer.sizes;
  // The standard size when offered, otherwise the first.
  const size = sizes.some(s => s.size === settings.size) ? settings.size : sizes[0]?.size ?? settings.size;
  const chosen = sizes.find(s => s.size === size);
  return <li className="agent-computer-choice">
    <span className="agent-app-text"><strong>Computer</strong>
      <small>{name}’s own Linux computer for files and programs: read and convert documents, work through data, run scripts</small>
      <span className="agent-app-kinds"><span className="agent-kind change">Changes files</span><span className="agent-kind">Off unless you switch it on</span></span>
      {settings.on && <label className="ui-field">Size<select value={size} onChange={event => change({ ...settings, size: event.target.value })}>
        {sizes.map(s => <option key={s.size} value={s.size}>{sizeNames[s.size] ?? s.size} · {perHour(s.usd_per_hour)}</option>)}
      </select></label>}
      {settings.on && chosen && <small>{sizeFacts(chosen)}</small>}
      <small>{settings.on ? `It is made the first time ${name} needs it; nothing is charged until then. Choose what it may do in Controls.`
        : 'It keeps its files between tasks and sleeps when not in use.'}</small>
    </span>
    <label className="ui-switch"><input type="checkbox" role="switch" checked={settings.on}
      onChange={event => change({ ...settings, size, on: event.target.checked })} />
      <span>Use a computer</span></label>
  </li>;
}
