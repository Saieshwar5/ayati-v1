import { useId, useRef } from 'react';
import { Button } from '../ui';
import { workspaceUrl } from '../../workspace-context';

export const workspaceSections = ['Overview', 'Instructions', 'Connections', 'Automations', 'Settings'] as const;
export type WorkspaceSection = typeof workspaceSections[number];
// Older links and saved choices: Tools became Connections, Environment became
// Settings, Activity is part of Overview, and Schedules was Automations' first name.
const formerNames: Record<string, string> = { schedules: 'automations', tools: 'connections', environment: 'settings', activity: 'overview' };
/** A saved or linked section name, under its current or an earlier name. */
export function sectionNamed(value: string | null | undefined): WorkspaceSection | undefined {
  const lower = value?.toLowerCase();
  const name = lower ? formerNames[lower] ?? lower : lower;
  return workspaceSections.find(section => section.toLowerCase() === name);
}
/** A link that opens one agent on one tab, e.g. from Activity or Chat; `action`
 * ("connection/operation") picks out one rule on the Connections tab. */
export const agentSectionUrl = (agent: string, section: WorkspaceSection, action?: string) =>
  workspaceUrl(`/?view=agents&selected=${encodeURIComponent(agent)}&section=${section.toLowerCase()}${action ? `&action=${encodeURIComponent(action)}` : ''}`);
export function WorkspaceTabs({ selected, select }: { selected: WorkspaceSection; select: (section: WorkspaceSection) => void }) {
  const id = useId();
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  return <div className="agent-workspace-tabs" role="tablist" aria-label="Agent workspace sections">
    {workspaceSections.map((section, index) => <Button key={section} ref={node => { buttons.current[index] = node; }}
      id={`${id}-${section}`} role="tab" aria-controls="agent-workspace-panel" aria-selected={selected === section}
      tabIndex={selected === section ? 0 : -1} onClick={() => select(section)} onKeyDown={event => {
        const next = event.key === 'ArrowRight' ? (index + 1) % workspaceSections.length
          : event.key === 'ArrowLeft' ? (index + workspaceSections.length - 1) % workspaceSections.length
            : event.key === 'Home' ? 0 : event.key === 'End' ? workspaceSections.length - 1 : -1;
        if (next >= 0) { event.preventDefault(); select(workspaceSections[next]); buttons.current[next]?.focus(); }
      }}>{section}</Button>)}
  </div>;
}
