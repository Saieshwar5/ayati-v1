import { useId, useRef } from 'react';
import { Button } from '../ui';
import { workspaceUrl } from '../../workspace-context';

export const workspaceSections = ['Overview', 'Activity', 'Instructions', 'Tools', 'Automations', 'Environment'] as const;
export type WorkspaceSection = typeof workspaceSections[number];
/** A saved or linked section name; "Schedules" was the Automations tab's old name. */
export function sectionNamed(value: string | null | undefined): WorkspaceSection | undefined {
  const name = value?.toLowerCase() === 'schedules' ? 'automations' : value?.toLowerCase();
  return workspaceSections.find(section => section.toLowerCase() === name);
}
/** A link that opens one agent on one tab, e.g. from Activity or Chat. */
export const agentSectionUrl = (agent: string, section: WorkspaceSection) =>
  workspaceUrl(`/?view=agents&selected=${encodeURIComponent(agent)}&section=${section.toLowerCase()}`);
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
