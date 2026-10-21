import { useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { Button } from '../ui';
import { decodeMentions, editMentions, encodeMentions, type Mention } from './mentions';
import './composer.css';
export type ComposerProps = {
  value: string; change: (value: string) => void; send: () => void;
  maxLength?: number; disabled?: boolean; sendDisabled?: boolean; placeholder?: string; label?: string;
  candidates?: Mention[]; media?: ReactNode; attachments?: ReactNode; trailing?: ReactNode;
};
/** Shared UI; each conversation adapter owns its authorized transport and durable draft. */
export function Composer({ value, change, send, disabled = false, sendDisabled = false, maxLength = 12000, placeholder = 'Message…', label = 'Message', candidates = [], media, attachments, trailing }: ComposerProps) {
  const input = useRef<HTMLTextAreaElement>(null);
  const restoreFocus = useRef(false);
  const submit = () => { if (!disabled && !sendDisabled) { restoreFocus.current = true; send(); } };
  const [caret, setCaret] = useState(0), [active, setActive] = useState(0), [dismissed, setDismissed] = useState(false);
  const menuId = useId(), restore = useRef<number | undefined>(undefined);
  const { text, mentions } = decodeMentions(value);
  const query = /(?:^|\s)@([^@\n]{0,60})$/.exec(text.slice(0, caret));
  const start = query ? caret-query[1].length-1 : -1;
  const choices = query && !dismissed && !mentions.some(m => start >= m.start && start < m.end)
    ? candidates.filter(m => m.name.toLocaleLowerCase().includes(query[1].toLocaleLowerCase())).slice(0,8) : [];
  const selected = Math.min(active, Math.max(0, choices.length-1));
  useLayoutEffect(() => {
    const node = input.current; if (!node) return;
    node.style.height = 'auto'; node.style.height = `${Math.min(node.scrollHeight, 160)}px`;
    if (restore.current !== undefined) { node.focus(); node.setSelectionRange(restore.current, restore.current); restore.current = undefined; }
    if (!disabled && restoreFocus.current) { restoreFocus.current = false; node.focus(); }
  }, [value, disabled, placeholder]);
  useEffect(() => {
    const node = input.current;
    if (!node || typeof ResizeObserver === 'undefined') return;
    let width = node.clientWidth;
    const observer = new ResizeObserver(() => {
      if (width === node.clientWidth) return;
      width = node.clientWidth; node.style.height = 'auto'; node.style.height = `${Math.min(node.scrollHeight,160)}px`;
    });
    observer.observe(node); return () => observer.disconnect();
  }, []);
  const choose = (mention: Mention) => {
    const name = mention.name.replace(/[\]\n]/g, ' '), inserted = `@${name} `;
    const next = text.slice(0,start)+inserted+text.slice(caret);
    const base = decodeMentions(editMentions(value,next));
    const nextMentions = [...base.mentions, { ...mention, name, start, end: start+inserted.length-1 }];
    restore.current = start+inserted.length; setCaret(restore.current); setDismissed(true);
    change(encodeMentions(next,nextMentions));
  };
  return <form className="message-composer" aria-label="Message composer" onSubmit={event => { event.preventDefault(); submit(); }}>
    {choices.length > 0 && <div className="mention-menu" id={menuId} role="listbox" aria-label="Mention suggestions">
      {choices.map((item,index) => <button type="button" role="option" id={`${menuId}-${index}`} aria-selected={index === selected} key={`${item.kind}:${item.id}`} onMouseDown={event => event.preventDefault()} onClick={() => choose(item)}>
        <span>{item.kind === 'agent' ? '✧' : '@'}</span><strong>{item.name}</strong><small>{item.kind === 'agent' ? 'Agent' : 'Person'}</small>
      </button>)}
    </div>}
    <textarea ref={input} aria-label={label} aria-autocomplete={candidates.length ? 'list' : undefined} aria-controls={choices.length ? menuId : undefined} aria-activedescendant={choices.length ? `${menuId}-${selected}` : undefined}
      rows={1} placeholder={placeholder} value={text} disabled={disabled} maxLength={maxLength}
      onBlur={event => { if (!event.currentTarget.form?.contains(event.relatedTarget as Node | null)) setDismissed(true); }}
      onSelect={event => setCaret(event.currentTarget.selectionStart)}
      onChange={event => { setCaret(event.target.selectionStart); setDismissed(false); setActive(0); change(editMentions(value,event.target.value)); }}
      onKeyDown={event => {
        if (event.nativeEvent.isComposing) return;
        if (choices.length && ['ArrowDown','ArrowUp','Enter','Tab','Escape'].includes(event.key)) {
          event.preventDefault();
          if (event.key === 'Escape') setDismissed(true);
          else if (event.key === 'Enter' || event.key === 'Tab') choose(choices[selected]);
          else setActive((selected+(event.key === 'ArrowDown' ? 1 : choices.length-1)) % choices.length);
        } else if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); submit(); }
      }} />
    {attachments && <div className="composer-attachments">{attachments}</div>}
    <div className="composer-toolbar"><div className="composer-media">{media}</div><div className="composer-actions">{trailing}<Button primary type="submit" aria-label="Send message" disabled={disabled || sendDisabled}>↑</Button></div></div>
  </form>;
}
