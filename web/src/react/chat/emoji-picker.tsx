import { useEffect, useId, useRef, useState } from 'react';
import { Button } from '../ui';
import { searchEmoji } from './emoji-list';

/** Composer button that inserts an emoji into the text being written. */
export function EmojiPicker({ insert, disabled }: { insert: (emoji: string) => void; disabled?: boolean }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const root = useRef<HTMLSpanElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const id = useId();
  const results = searchEmoji(query);
  const close = (refocus: boolean) => { setOpen(false); setQuery(''); if (refocus) toggle.current?.focus(); };

  // Clicking anywhere else closes the picker.
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) close(false); };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [open]);

  return <span className="emoji-picker" ref={root}>
    <Button ref={toggle} className="ui-icon composer-emoji" aria-label="Add emoji" title="Add emoji" aria-expanded={open} aria-controls={open ? id : undefined}
      disabled={disabled} onClick={() => (open ? close(false) : setOpen(true))}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true">
        <circle cx="12" cy="12" r="9" /><path d="M8.5 14.5s1.3 1.8 3.5 1.8 3.5-1.8 3.5-1.8" /><path d="M9 9.5h.01M15 9.5h.01" strokeWidth="2.4" strokeLinecap="round" />
      </svg>
    </Button>
    {open && <div className="emoji-panel" id={id} role="dialog" aria-label="Emoji" onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); close(true); } }}>
      <input type="search" autoFocus aria-label="Search emoji" placeholder="Search emoji" value={query} onChange={event => setQuery(event.target.value)} />
      <div className="emoji-grid" role="group" aria-label={query ? 'Matching emoji' : 'Frequently used'}>
        {results.map(item => <button type="button" key={item.emoji} aria-label={item.words.split(' ')[0]} title={item.words.split(' ').slice(0, 2).join(' ')}
          onClick={() => { insert(item.emoji); close(false); }}>{item.emoji}</button>)}
        {!results.length && <p className="ui-help">No emoji match “{query}”.</p>}
      </div>
    </div>}
  </span>;
}
