import { useEffect, useId, useRef, useState, type ComponentProps, type ReactNode, type RefObject } from 'react';
import { createPortal } from 'react-dom';

export function Brand({ className = '', symbol = false }: { className?: string; symbol?: boolean }) {
  return <span className={`cuckoo-brand ${symbol ? 'cuckoo-symbol' : 'cuckoo-logo'} ${className}`} role="img" aria-label="Cuckoo" />;
}

export function Button({ primary = false, className = '', type = 'button', ...props }: ComponentProps<'button'> & { primary?: boolean }) {
  return <button {...props} type={type} className={`ui-button${primary ? ' primary' : ''} ${className}`} />;
}

export function Tabs({ sections }: { sections: { title: string; content: ReactNode }[] }) {
  const id = useId();
  const [selected, select] = useState(0);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  return <><div className="ui-tabs" role="tablist">{sections.map(({ title }, index) =>
    <Button key={title} ref={node => { buttons.current[index] = node; }} id={`${id}-${index}`} role="tab"
      aria-controls={`${id}-panel-${index}`} aria-selected={selected === index} tabIndex={selected === index ? 0 : -1}
      onClick={() => select(index)} onKeyDown={event => {
        const next = event.key === 'ArrowRight' ? (index + 1) % sections.length
          : event.key === 'ArrowLeft' ? (index + sections.length - 1) % sections.length
          : event.key === 'Home' ? 0 : event.key === 'End' ? sections.length - 1 : -1;
        if (next >= 0) { event.preventDefault(); select(next); buttons.current[next]?.focus(); }
      }}>{title}</Button>)}</div>{sections.map(({ title, content }, index) =>
    <section key={title} role="tabpanel" id={`${id}-panel-${index}`} aria-labelledby={`${id}-${index}`} hidden={selected !== index} tabIndex={0}>{content}</section>)}</>;
}

export function Dialog({ title, drawer = false, busy = false, status = '', close, children, footer, initialFocus }: {
  title: string; drawer?: boolean; busy?: boolean; status?: string; close: () => void; children: ReactNode; footer?: ReactNode; initialFocus?: RefObject<HTMLElement | null>;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const id = useId();
  const latest = useRef(close); latest.current = close;
  useEffect(() => {
    const node = dialog.current!;
    const previous = document.activeElement as HTMLElement | null;
    node.showModal();
    initialFocus?.current?.focus();
    return () => { node.close(); if (previous?.isConnected) previous.focus(); };
  }, [initialFocus]);
  return createPortal(<dialog ref={dialog} className={`ui-dialog${drawer ? ' ui-drawer' : ''}`} aria-labelledby={id} aria-busy={busy}
    onCancel={event => { event.preventDefault(); event.stopPropagation(); if (!busy) latest.current(); }}>
    <header className="ui-dialog-header"><h2 id={id}>{title}</h2><Button className="ui-icon" aria-label="Close" disabled={busy} onClick={close}>×</Button></header>
    <div className="ui-dialog-body"><fieldset className="ui-fieldset" disabled={busy}>{children}</fieldset></div>
    <footer className="ui-dialog-footer"><p className="ui-status" role="status">{status}</p>{footer}</footer>
  </dialog>, document.body);
}
