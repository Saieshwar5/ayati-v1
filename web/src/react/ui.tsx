import { createContext, useContext, useEffect, useId, useRef, useState, type ComponentProps, type ReactNode, type RefObject } from 'react';
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

type DialogProps = {
  title: string; drawer?: boolean; busy?: boolean; status?: string; close: () => void; back?: () => void;
  children: ReactNode; footer?: ReactNode; initialFocus?: RefObject<HTMLElement | null>;
};

/** Where a Dialog shows. A host with a reading pane (Activity → Needs me) provides this,
 * and the first Dialog inside it renders in place beside the list instead of over the
 * page. `settled` runs after each write in that dialog finishes, so the host re-reads;
 * `after` is the host's own content, shown under the dialog's and scrolling with it. */
export const DialogPlacement = createContext<{ settled?: () => void; after?: ReactNode } | undefined>(undefined);

/** A native modal dialog or drawer. `back` adds a back arrow before the title, for a
 * second step inside the same panel. */
export function Dialog(props: DialogProps) {
  const placement = useContext(DialogPlacement);
  return placement ? <PaneDialog {...props} settled={placement.settled} after={placement.after} /> : <ModalDialog {...props} />;
}

function DialogParts({ id, title, busy = false, status = '', close, back, children, footer, after }: DialogProps & { id: string; after?: ReactNode }) {
  return <>
    <header className="ui-dialog-header">
      {back && <Button className="ui-icon ui-dialog-back" aria-label="Back" disabled={busy} onClick={back}>‹</Button>}
      <h2 id={id}>{title}</h2><Button className="ui-icon" aria-label="Close" disabled={busy} onClick={close}>×</Button></header>
    <div className="ui-dialog-body"><fieldset className="ui-fieldset" disabled={busy}>{children}</fieldset>{after}</div>
    <footer className="ui-dialog-footer"><p className="ui-status" role="status">{status}</p>{footer}</footer>
  </>;
}

function ModalDialog(props: DialogProps) {
  const { drawer = false, busy = false, close, initialFocus } = props;
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
    <DialogParts {...props} id={id} />
  </dialog>, document.body);
}

/** The same dialog shown in place. It takes no focus and covers nothing; dialogs it
 * opens itself are ordinary modal ones. */
function PaneDialog({ settled, after, ...props }: DialogProps & { settled?: () => void; after?: ReactNode }) {
  const { busy = false, close } = props;
  const id = useId();
  const was = useRef(busy);
  const latest = useRef(settled); latest.current = settled;
  useEffect(() => { if (was.current && !busy) latest.current?.(); was.current = busy; }, [busy]);
  return <section className="ui-pane" aria-labelledby={id} aria-busy={busy}
    onKeyDown={event => { if (event.key === 'Escape' && !busy && !event.defaultPrevented) { event.stopPropagation(); close(); } }}>
    <DialogPlacement.Provider value={undefined}><DialogParts {...props} id={id} after={after} /></DialogPlacement.Provider>
  </section>;
}
