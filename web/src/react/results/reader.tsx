import { useEffect, useRef, useState } from 'react';
import { messageHtml } from '../../markdown';
import { Button, Dialog } from '../ui';
import { errorMessage } from '../requests';
import { denied, query, type ResultSource, type ResponseReference, type SavedResponse } from './model';
export function ResponseReader({ id, source, close }: { id: string; source: ResultSource; close: () => void }) {
  const [data, setData] = useState<SavedResponse>();
  const [latest, setLatest] = useState<ResponseReference>();
  const [error, setError] = useState('');
  const [status, setStatus] = useState('');
  const [busy, setBusy] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [retry, setRetry] = useState(0);
  const body = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const abort = new AbortController(); let loading = false, epoch = 0, sourceScope: string | null | undefined;
    setData(undefined); setLatest(undefined); setError(''); setBusy(true);
    const load = async (metadata: boolean) => {
      if (loading || abort.signal.aborted) return;
      loading = true; const request = ++epoch;
      try {
        const next = await source.read<SavedResponse>(query(source.response(id), { metadata }), abort.signal);
        if (abort.signal.aborted || request !== epoch) return;
        sourceScope=next.source_scope;
        if (metadata) setLatest(next);
        else { setData(next); setLatest(next); setStatus(''); }
        setError('');
      } catch (failure) {
        if (abort.signal.aborted || request !== epoch) return;
        if (denied(failure)) { setData(undefined); setLatest(undefined); }
        setError(errorMessage(failure));
      } finally { if (request === epoch) { loading = false; if (!abort.signal.aborted) setBusy(false); } }
    };
    void load(false);
    const check = (scope?: string | null) => {
      if (scope !== undefined) {
        if (scope !== null && sourceScope !== undefined && sourceScope !== null && scope !== sourceScope) return;
        if (scope !== null && sourceScope === null) return;
        epoch++; loading=false; setData(undefined); setLatest(undefined); setError(''); setBusy(true); void load(false);
      } else void load(true);
    };
    const timer = window.setInterval(() => { if (!document.hidden) check(); }, 5000);
    const unsubscribe = source.subscribe?.(check);
    return () => { abort.abort(); epoch++; window.clearInterval(timer); unsubscribe?.(); };
  }, [id, source, retry]);
  const newer = data && latest && (latest.version !== data.version || latest.state !== data.state);
  const update = () => { setRetry(value => value + 1); if (body.current) body.current.scrollTop = 0; };
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
