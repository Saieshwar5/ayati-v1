import { useRef, useState } from 'react';
import { messageHtml } from '../../markdown';
import { Button, Dialog } from '../ui';
import { query, type ResultSource } from './model';
import { useSavedResponse } from './use-saved-response';
export function ResponseReader({ id, source, close }: { id: string; source: ResultSource; close: () => void }) {
  const { data, latest, error, busy, update: reload } = useSavedResponse(id, source);
  const [status, setStatus] = useState('');
  const [expanded, setExpanded] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  const newer = data && latest && (latest.version !== data.version || latest.state !== data.state);
  const update = () => { reload(); setStatus(''); if (body.current) body.current.scrollTop = 0; };
  return <Dialog title="Agent response" close={close} status={status}>
    <div className={`result-reader${expanded ? ' result-reader-expanded' : ''}`}>
      <div className="result-toolbar"><span>{data?.state === 'draft' ? 'Saved draft · still working' : data?.state === 'interrupted' ? 'Saved partial response · work interrupted' : data ? 'Saved response' : error ? 'Response unavailable' : 'Loading response…'}</span><Button onClick={() => setExpanded(value => !value)}>{expanded ? 'Reduce' : 'Expand'}</Button>
        <Button disabled={!data} onClick={() => { if (data) void navigator.clipboard.writeText(data.body).then(() => setStatus('Response copied.')).catch(() => setStatus('Copy failed. Select and copy the response text.')); }}>Copy</Button>
        {data?.state === 'complete' && <a className="ui-button" href={source.href(query(source.response(id), { download: true, version: data.version }))} download>Download Markdown</a>}
      </div>
      {data?.state !== 'complete' && data && <p className="result-note">This text is saved so far. It may be incomplete; a Markdown download is available when the response is complete.</p>}
      {(data?.state === 'draft' || newer) && <div className="result-update" role="status"><span>{newer ? latest.state === 'complete' ? 'The complete response is ready.' : 'A newer saved version is available.' : 'Your reading view stays unchanged.'}</span><Button disabled={!newer || busy} onClick={update}>Load latest saved response</Button></div>}
      {error && <p role="alert" className="chat-error">{error} <Button disabled={busy} onClick={update}>Retry</Button></p>}
      {!data && !error && <p role="status">Loading saved response…</p>}
      {data && <div ref={body} tabIndex={0} aria-label="Saved response content" className="result-body message-markdown" dangerouslySetInnerHTML={{ __html: messageHtml(data.body) }} />}
    </div>
  </Dialog>;
}
