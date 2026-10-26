import { useId, useRef } from 'react';
import { Button } from '../ui';

export const workspaceSections = ['Overview', 'Work', 'Knowledge', 'Access', 'Automation', 'Environment', 'Activity'] as const;
export type WorkspaceSection = typeof workspaceSections[number];
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
