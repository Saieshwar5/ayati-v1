import { ModePicker } from '../mode-picker';
import type { ListedTool, ToolChoice } from './model';

type Choices = Record<string, ToolChoice & { on: boolean }>;

/** Step 2 of connecting a system: each tool with what it does, Reads or
 * Changes, and Allow / Ask first / Block. A tool Cuckoo can't use says why. */
export function ReviewTools({ tools, choices, change, held = [] }: {
  tools: ListedTool[]; choices: Choices; change: (next: Choices) => void; held?: string[];
}) {
  const on = Object.values(choices).filter(c => c.on).length;
  const set = (name: string, patch: Partial<ToolChoice & { on: boolean }>) => change({ ...choices, [name]: { ...choices[name], ...patch } });
  return <div className="system-review">
    <p className="ui-help">{on} of at most 20 tools switched on. Reads start on Allow; changes start on Ask first. You can make any rule stricter later.</p>
    <ul className="system-tools">
      {tools.map(tool => {
        const choice = choices[tool.name];
        if (!tool.supported || !choice) return <li key={tool.name} className="system-tool unsupported">
          <span><strong>{tool.title}</strong><small className="system-tool-name">{tool.name}</small>
            <small>Not supported: {tool.reason}</small></span>
        </li>;
        return <li key={tool.name} className={`system-tool${held.includes(tool.name) ? ' changed' : ''}`}>
          <label className="ui-switch"><input type="checkbox" role="switch" checked={choice.on}
            disabled={!choice.on && on >= 20} onChange={event => set(tool.name, { on: event.target.checked })} />
            <span className="visually-hidden">Use {tool.title}</span></label>
          <span><strong>{tool.title}</strong><small className="system-tool-name">{tool.name}</small>
            {tool.description && <small>{tool.description}</small>}
            {held.includes(tool.name) && <small className="system-tool-held">Changed since it was reviewed. It is held until you save this review.</small>}</span>
          <label className="ui-field system-kind">Kind<select value={choice.kind} disabled={!choice.on}
            onChange={event => set(tool.name, { kind: event.target.value as 'read' | 'write' })}>
            <option value="read">Reads</option><option value="write">Changes</option></select></label>
          <ModePicker label={`${tool.title} rule`} value={choice.rule} disabled={!choice.on} change={rule => set(tool.name, { rule })} />
          {tool.fields.includes('subject') && <label className="system-subject"><input type="checkbox" checked={Boolean(choice.subject)} disabled={!choice.on}
            onChange={event => set(tool.name, { subject: event.target.checked })} /><span>Needs the subject<small>Cuckoo fills “subject” from the event and hides it from the agent, so it can only reach that subject’s data. It can’t be used from Chat.</small></span></label>}
        </li>;
      })}
    </ul>
  </div>;
}
export const chosen = (choices: Choices): ToolChoice[] =>
  Object.values(choices).filter(c => c.on).map(({ name, kind, rule, subject }) => ({ name, kind, rule, ...(subject ? { subject } : {}) }));
