import './tools.css';
import { allowedFor, modes, type Mode } from './control-model';

/** Allow / Ask first / Block for one action. Options looser than `ceiling` are
 * shown but cannot be chosen (designs/agents-simplified 9). */
export function ModePicker({ label, value, ceiling = 'allow', disabled, change }: {
  label: string; value: Mode; ceiling?: Mode; disabled?: boolean; change: (mode: Mode) => void;
}) {
  return <div className="mode-picker" role="radiogroup" aria-label={label}>
    {(Object.keys(modes) as Mode[]).map(mode => <label key={mode} className={`mode-option ${mode}${value === mode ? ' chosen' : ''}`}>
      <input type="radio" name={label} value={mode} checked={value === mode}
        disabled={disabled || !allowedFor(ceiling, mode)} onChange={() => change(mode)} />
      {modes[mode]}
    </label>)}
  </div>;
}
