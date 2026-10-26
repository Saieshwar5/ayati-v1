import { useId, useLayoutEffect, useRef, useState } from 'react';
import { messageHtml } from '../../markdown';
import { Button } from '../ui';
import { CornerIcon } from './corner-icon';
import { responsePreview, query, type ResponseReference, type ResultSource } from './model';
import { useSavedResponse } from './use-saved-response';
import './results.css';

type Props = { id: string; text: string; reference?: ResponseReference; source?: ResultSource;
  reading?: (key: string, open: boolean) => void; openReader?: () => void };

export function InlineResponse({ id, text, reference, source, reading, openReader }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [overflow, setOverflow] = useState(false);
  const content = useRef<HTMLDivElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const restore = useRef(false);
  const key = useId();
  const bodyId = useId();
  const preview = responsePreview(text, reference);
  useLayoutEffect(() => {
    if (expanded || !content.current) return;
    const node = content.current;
    const measure = () => setOverflow(node.scrollHeight > node.clientHeight + 1);
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure); observer.observe(node);
    return () => observer.disconnect();
  }, [expanded, preview.text]);
  useLayoutEffect(() => {
    reading?.(key, expanded);
    return () => reading?.(key, false);
  }, [expanded, key, reading]);
  useLayoutEffect(() => {
    if (!expanded && restore.current) {
      restore.current = false;
      box.current?.scrollIntoView?.({ block: 'nearest' });
    }
  }, [expanded]);
  const readable = preview.long || overflow;
  const toggle = () => { restore.current = expanded; setExpanded(value => !value); };
  return <div ref={box} className={`chat-response${readable ? ' chat-response-long' : ''}${expanded ? ' chat-response-expanded' : ''}`}>
    {expanded && <button type="button" className="response-corner response-collapse-top" aria-label="Collapse response from top"
      title="Collapse response" aria-expanded="true" aria-controls={bodyId} onClick={toggle}><CornerIcon /></button>}
    {expanded && source ? <InlineBody id={id} source={source} bodyId={bodyId} openReader={openReader} /> :
      <div id={bodyId} ref={content} className="message-markdown response-excerpt" dangerouslySetInnerHTML={{ __html: messageHtml(preview.text) }} />}
    {readable && <button type="button" className="response-corner" aria-label={expanded ? 'Collapse response' : 'Expand response'}
      title={reference?.available === false ? 'Response exceeds the reading size limit' : expanded ? 'Collapse response' : 'Expand response'}
      disabled={!expanded && (!source || reference?.available === false)} aria-expanded={expanded} aria-controls={bodyId} onClick={toggle}>
      <CornerIcon collapsed={!expanded} />
    </button>}
  </div>;
}

function InlineBody({ id, source, bodyId, openReader }: { id: string; source: ResultSource; bodyId: string; openReader?: () => void }) {
  const { data, latest, error, busy, update } = useSavedResponse(id, source);
  const [status, setStatus] = useState('');
  const newer = data && latest && (latest.version !== data.version || latest.state !== data.state);
  return <div id={bodyId}>
    {(data?.state === 'draft' || newer) && <div className="inline-response-update" role="status">
      <span>{newer ? 'A newer saved response is available.' : 'Saved draft · reading view stays unchanged.'}</span>
      <Button disabled={!newer || busy} onClick={update}>Load latest saved response</Button>
    </div>}
    {data?.state === 'interrupted' && <p className="result-note">Saved partial response · work interrupted.</p>}
    {error && <p role="alert">{error} <Button disabled={busy} onClick={update}>Retry</Button></p>}
    {!data && !error && <p role="status">Loading saved response…</p>}
    {data && <>
      <div className="message-markdown inline-response-body" aria-label="Saved response content" dangerouslySetInnerHTML={{ __html: messageHtml(data.body) }} />
      <div className="inline-response-tools">
        <button type="button" aria-label="Copy response" title="Copy response" onClick={() => void navigator.clipboard.writeText(data.body)
          .then(() => setStatus('Response copied.')).catch(() => setStatus('Copy failed. Select and copy the response text.'))}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><rect x="8" y="8" width="12" height="13" rx="2"/><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3"/></svg>
        </button>
        {data.state === 'complete' && <a aria-label="Download Markdown" title="Download Markdown" href={source.href(query(source.response(id), { download: true, version: data.version }))} download>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><path d="M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5"/></svg>
        </a>}
        {openReader && <button type="button" aria-label="Open response reader" title="Open response reader" onClick={openReader}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M14 3v18"/></svg>
        </button>}
      </div>
    </>}
    {status && <p role="status">{status}</p>}
  </div>;
}
