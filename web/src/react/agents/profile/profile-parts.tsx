import { useEffect, useRef, useState, type ReactNode } from 'react';

const paths = {
  rules: <><path d="M9 6h11M9 12h11M9 18h7" /><path d="M4.5 6h.01M4.5 12h.01M4.5 18h.01" strokeWidth="2.4" /></>,
  skill: <><path d="M11 3l1.9 5.1L18 10l-5.1 1.9L11 17l-1.9-5.1L4 10l5.1-1.9z" /><path d="M18.5 15.5l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8z" /></>,
  book: <><path d="M5 4h11a3 3 0 0 1 3 3v13H8a3 3 0 0 1-3-3z" /><path d="M5 17a3 3 0 0 1 3-3h11" /></>,
  plug: <path d="M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0zM12 17v4" />,
  clock: <><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></>,
  bolt: <path d="M13 3 5 13h6l-1 8 8-10h-6z" />,
  people: <><circle cx="9" cy="8" r="3.2" /><path d="M3 20a6 6 0 0 1 12 0" /><circle cx="17.5" cy="9" r="2.5" /><path d="M17 14.5a4.5 4.5 0 0 1 4 4.5" /></>,
  chip: <><rect x="6" y="6" width="12" height="12" rx="2" /><path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4" /></>,
  note: <><path d="M5 4h14v11l-5 5H5z" /><path d="M14 20v-5h5" /></>,
  doc: <><path d="M7 3h7l5 5v13H7z" /><path d="M14 3v5h5M10 13h6M10 17h6" /></>,
  bulb: <path d="M9 18h6M10 21h4M12 3a6 6 0 0 0-4 10.5c.7.7 1 1.5 1 2.5h6c0-1 .3-1.8 1-2.5A6 6 0 0 0 12 3z" />,
  monitor: <><rect x="3" y="4" width="18" height="12" rx="2" /><path d="M8 20h8M12 16v4" /></>,
  server: <><rect x="4" y="4" width="16" height="7" rx="2" /><rect x="4" y="13" width="16" height="7" rx="2" /><path d="M8 7.5h.01M8 16.5h.01" strokeWidth="2.4" /></>,
  globe: <><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" /></>,
  lock: <><rect x="5" y="11" width="14" height="9" rx="2" /><path d="M8 11V8a4 4 0 0 1 8 0v3" /></>,
  out: <path d="M8 16 16 8M9.5 8H16v6.5" />,
  plus: <path d="M12 5v14M5 12h14" />,
  pencil: <><path d="M4 20h4L19 9l-4-4L4 16z" /><path d="m13 7 4 4" /></>,
  x: <path d="M6 6l12 12M18 6 6 18" />,
  check: <path d="m5 12 5 5 9-10" />,
  ask: <><circle cx="12" cy="12" r="9" /><path d="M9.6 9.2a2.5 2.5 0 1 1 3.6 2.3c-.8.4-1.2 1-1.2 1.9M12 17v.01" /></>,
  spin: <path d="M12 3a9 9 0 1 0 9 9" />,
  memory: <><circle cx="12" cy="8" r="3.5" /><path d="M5 20a7 7 0 0 1 14 0" /></>,
};
export type ProfileIconName = keyof typeof paths;

/** The page's small line icons; decorative, so hidden from screen readers. */
export function ProfileIcon({ name, size = 16 }: { name: ProfileIconName; size?: number }) {
  return <svg className="profile-icon" viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{paths[name]}</svg>;
}

/** One row of the page: what it is about on the left, the things themselves beside it. */
export function Row({ icon, label, children, action }: { icon: ProfileIconName; label: string; children: ReactNode; action?: ReactNode }) {
  return <section className="profile-row" aria-label={label}>
    <h3 className="profile-row-label"><ProfileIcon name={icon} />{label}</h3>
    <div className="profile-row-body">{children}</div>
    {action}
  </section>;
}

/** A thing on the page. With `open` it is a button that opens it; `current` marks the one whose panel is showing. */
export function Chip({ children, open, current = false, label }: { children: ReactNode; open?: () => void; current?: boolean; label?: string }) {
  return open
    ? <button type="button" className="profile-chip" aria-label={label} aria-current={current ? 'true' : undefined} onClick={open}>{children}</button>
    : <span className="profile-chip">{children}</span>;
}

export type PlusItem = { icon: ProfileIconName; label: string; pick: () => void };

/**
 * The ＋ at the end of a row. `word` names what it adds while the row is empty,
 * so an empty page says what belongs where. With one item it adds at once; with
 * several a small menu asks which.
 */
export function Plus({ label, items, word }: { label: string; items: PlusItem[]; word?: string }) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null), toggle = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    wrap.current?.querySelector<HTMLButtonElement>('[role=menuitem]')?.focus();
    const outside = (event: MouseEvent) => { if (!wrap.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, [open]);
  if (!items.length) return null;
  const single = items.length === 1;
  const pick = (item: PlusItem) => { setOpen(false); toggle.current?.focus(); item.pick(); };
  return <span ref={wrap} className="profile-plus" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false); }}>
    <button ref={toggle} type="button" className="profile-chip profile-add" aria-label={label} aria-haspopup={single ? undefined : 'menu'} aria-expanded={single ? undefined : open}
      onClick={() => single ? items[0].pick() : setOpen(!open)}><ProfileIcon name="plus" size={14} />{word}</button>
    {open && <div className="chat-message-menu profile-plus-menu" role="menu" aria-label={label}
      onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); setOpen(false); toggle.current?.focus(); } }}>
      {items.map(item => <button key={item.label} type="button" role="menuitem" onClick={() => pick(item)}><ProfileIcon name={item.icon} />{item.label}</button>)}
    </div>}
  </span>;
}

/** Worded ＋ chips for an empty row: one per kind of thing it can hold. */
export function AddWords({ items }: { items: PlusItem[] }) {
  return <>{items.map(item => <button key={item.label} type="button" className="profile-chip profile-add" aria-label={`Add ${item.label}`} onClick={item.pick}><ProfileIcon name="plus" size={14} />{item.label}</button>)}</>;
}
