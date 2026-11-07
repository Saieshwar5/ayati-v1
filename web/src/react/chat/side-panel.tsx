import { useLayoutEffect, useRef, type ReactNode } from 'react';
import { Button } from '../ui';

/**
 * The panel beside the conversation, inside the workspace frame (D245): channel
 * Details and profiles. It is not modal, so the conversation stays readable.
 * Focus moves to its title on open and back to whatever opened it on close.
 */
export function SidePanel({ title, label, close, back, children }: {
  title: ReactNode; label: string; close: () => void;
  back?: { label: string; go: () => void }; children: ReactNode;
}) {
  const panel = useRef<HTMLElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  // A layout effect records the opener before children (like the archive review) move focus.
  useLayoutEffect(() => {
    const node = panel.current!;
    const previous = document.activeElement as HTMLElement | null;
    heading.current?.focus();
    return () => {
      // Hand focus back only when it was in the panel (or is now lost), so a
      // click elsewhere, like opening another conversation, keeps its focus.
      const active = document.activeElement;
      if (previous?.isConnected && previous !== document.body && (!active || active === document.body || node.contains(active))) previous.focus();
    };
  }, []);
  return <aside ref={panel} className="chat-side-panel" aria-label={label}
    onKeyDown={event => { if (event.key === 'Escape' && !event.defaultPrevented) { event.preventDefault(); close(); } }}>
    <header className="chat-side-panel-header">
      {back && <Button className="ui-icon chat-side-panel-back" aria-label={back.label} title={back.label} onClick={back.go}>‹</Button>}
      <h2 ref={heading} tabIndex={-1}>{title}</h2>
      <Button className="ui-icon" aria-label={`Close ${label}`} title="Close" onClick={close}>×</Button>
    </header>
    <div className="chat-side-panel-body">{children}</div>
  </aside>;
}
