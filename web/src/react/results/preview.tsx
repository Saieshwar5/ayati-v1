import { useLayoutEffect, useRef, useState } from 'react';
import { messageHtml } from '../../markdown';
import { Button } from '../ui';
import { responsePreview, type ResponseReference } from './model';
export function ResponsePreview({ id, text, reference, open }: { id: string; text: string; reference?: ResponseReference; open: (id: string) => void }) {
  const content = useRef<HTMLDivElement>(null);
  const [overflow, setOverflow] = useState(false);
  const preview = responsePreview(text, reference);
  useLayoutEffect(() => {
    const node = content.current!;
    const measure = () => setOverflow(node.scrollHeight > node.clientHeight + 1);
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure); observer.observe(node);
    return () => observer.disconnect();
  }, [preview.text]);
  const readable = preview.long || overflow;
  return <div className="response-preview">
    <div ref={content} className="message-markdown response-excerpt" dangerouslySetInnerHTML={{ __html: messageHtml(preview.text) }} />
    {readable && <div className="response-preview-actions"><small>{reference?.state === 'draft' ? 'Saved draft' : reference?.state === 'interrupted' ? 'Saved partial response' : 'Agent response'}{reference ? ` · ${reference.characters.toLocaleString()} characters` : ''}</small><Button disabled={reference?.available === false} onClick={() => open(id)}>{reference?.available === false ? 'Reader size limit reached' : 'Read response'}</Button></div>}
  </div>;
}
