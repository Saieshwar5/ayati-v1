import { useId, useLayoutEffect, useRef, useState } from 'react';
import { messageHtml } from '../../markdown';
import { CornerIcon } from '../results/corner-icon';
import './message-text.css';

/**
 * A person's words in a message. Text taller than the limit in message-text.css shows its
 * first lines until it is opened; the whole text is already here, so opening reads nothing.
 */
export function MessageText({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);
  const [long, setLong] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  const restore = useRef(false);
  const bodyId = useId();
  useLayoutEffect(() => {
    const node = body.current;
    if (!node) return;
    const measure = () => {
      // Clipped text overflows its box. Opened text, and a short own message that is still
      // inline (so a height limit cannot clip it yet), are measured against the limit itself.
      const limit = parseFloat(window.getComputedStyle(node).getPropertyValue('--chat-text-limit'));
      setLong(node.scrollHeight > node.clientHeight + 1 || node.getBoundingClientRect().height > limit + 1);
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [expanded, text]);
  useLayoutEffect(() => {
    if (expanded || !restore.current) return;
    restore.current = false;
    body.current?.scrollIntoView?.({ block: 'nearest' });
  }, [expanded]);
  const toggle = () => { restore.current = expanded; setExpanded(value => !value); };
  // Tabbing to a link or finding text in the hidden part scrolls the clipped box: open it instead.
  const revealed = () => {
    const node = body.current;
    if (expanded || !node?.scrollTop) return;
    node.scrollTop = 0;
    setExpanded(true);
  };
  const label = expanded ? 'Collapse message' : 'Show full message';
  return <>
    <div id={bodyId} ref={body} className={`message-markdown chat-text${long ? ' chat-text-long' : ''}${expanded ? ' chat-text-expanded' : ''}`}
      onScroll={revealed} dangerouslySetInnerHTML={{ __html: messageHtml(text) }} />
    {long && <div className="chat-text-more">
      <button type="button" aria-label={label} title={label} aria-expanded={expanded} aria-controls={bodyId} onClick={toggle}><CornerIcon collapsed={!expanded} /></button>
    </div>}
  </>;
}
