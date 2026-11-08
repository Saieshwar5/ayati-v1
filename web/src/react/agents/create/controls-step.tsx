import { useEffect, useState, type Dispatch, type SetStateAction } from 'react';
import { Button } from '../../ui';
import { errorMessage } from '../../requests';
import { modes, type Mode, type ToolConnection } from '../control-model';
import { connectorName, ToolAction } from '../tools-tab';
import { suggestControls, type ControlSuggestion, type ControlsRequest } from '../skills/writer-model';

/** The connectors chosen during create, any action made stricter, and what was
 * suggested for each action. */
export type ToolChoices = Record<string, { on: boolean; modes: Record<string, Mode>; suggested?: Record<string, ControlSuggestion> }>;
/** Which setup the suggestions were made for, and by whom. */
export type Suggested = { key: string; by: 'writer' | 'rules' };

/** Suggestions fill each action the person hasn't changed; `all` resets every one. */
function applySuggestions(choices: ToolChoices, list: ControlSuggestion[], all = false): ToolChoices {
  const next: ToolChoices = { ...choices };
  for (const [id, choice] of Object.entries(choices)) {
    if (!choice.on) continue;
    const suggested = Object.fromEntries(list.filter(item => item.connection === id).map(item => [item.operation, item]));
    const kept = { ...choice.modes };
    for (const [operation, item] of Object.entries(suggested)) {
      const before = choice.suggested?.[operation]?.mode;
      if (all || kept[operation] === undefined || kept[operation] === before) kept[operation] = item.mode;
    }
    next[id] = { ...choice, modes: kept, suggested };
  }
  return next;
}

/** Create step 5, last (designs/agent-built-skills 4): Allow / Ask first / Block
 * for every action of the chosen apps, suggested by the agent from its apps,
 * skills, ground rules and who can talk to it. Suggestions are never looser than
 * safe defaults or the admin's rule; Cuckoo enforces the result before every action. */
export function ControlsStep({ name, connections, choices, change, request, suggested, setSuggested }: {
  name: string; connections?: ToolConnection[]; choices: ToolChoices; change: Dispatch<SetStateAction<ToolChoices>>;
  request: Omit<ControlsRequest, 'connections'>; suggested?: Suggested; setSuggested: (next: Suggested) => void;
}) {
  const [problem, setProblem] = useState('');
  const display = name.trim() || 'this agent';
  const chosen = connections?.filter(connection => choices[connection.id]?.on) ?? [];
  const ids = chosen.map(connection => connection.id);
  const key = JSON.stringify({ ...request, connections: ids });
  const waiting = chosen.length > 0 && suggested?.key !== key;
  useEffect(() => {
    if (!waiting) return;
    const controller = new AbortController();
    setProblem('');
    suggestControls({ ...JSON.parse(key) }, controller.signal).then(result => {
      if (controller.signal.aborted) return;
      // Applied to the newest choices, so a change made while waiting is kept.
      change(old => applySuggestions(old, result.suggestions));
      setSuggested({ key, by: result.by });
    }).catch(error => { if (!controller.signal.aborted) setProblem(errorMessage(error)); });
    return () => controller.abort();
  }, [key, waiting, change, setSuggested]);
  if (!connections) return <p role="status">Loading connectors…</p>;
  const reset = () => change(applySuggestions(choices, chosen.flatMap(connection => Object.values(choices[connection.id].suggested ?? {})), true));
  return <div className="agent-create-tools">
    <div className="agent-tools-note" aria-live="polite">
      {!chosen.length ? `No apps chosen, so ${display} can’t act in any app. You can switch apps on later on the Tools tab.`
        : problem ? `Controls couldn’t be suggested: ${problem} Your admin’s rules apply until you change them.`
        : waiting ? `${display} is suggesting controls from its apps, skills, ground rules and who can talk to it…`
        : suggested?.by === 'writer' ? `${display} suggested these from its apps, skills, what you said and who can talk to it. They are never looser than Cuckoo’s safe defaults or your admin’s rules; anything looser is your choice. Cuckoo checks them before every action.`
        : 'Suggested from its skills and ground rules: actions no skill uses are blocked. Anything looser than your admin’s rules can’t be chosen; Cuckoo checks these before every action.'}
      {chosen.length > 0 && !waiting && !problem && <Button onClick={reset}>Reset to suggestions</Button>}
    </div>
    {chosen.map(connection => {
      const choice = choices[connection.id];
      return <article key={connection.id} className="agent-tool" aria-label={connectorName(connection)}>
        <header><span><strong>{connectorName(connection)}</strong>
          <small>{connection.operations.length} {connection.operations.length === 1 ? 'action' : 'actions'}</small></span></header>
        {connection.operations.map(operation => {
          const tip = choice.suggested?.[operation.id];
          return <ToolAction key={operation.id} operation={operation} disabled={false} value={choice.modes[operation.id] ?? operation.workspace_mode}
            note={tip && `Suggested ${modes[tip.mode]}: ${tip.reason}`}
            change={mode => change({ ...choices, [connection.id]: { ...choice, modes: { ...choice.modes, [operation.id]: mode } } })} />;
        })}
      </article>;
    })}
  </div>;
}
