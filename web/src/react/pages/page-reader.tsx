import { useMemo, useRef, useState } from 'react';
import '../../message-content.css';
import { Button, Dialog } from '../ui';
import { query, type ResultSource } from '../results/model';
import { useSavedResponse } from '../results/use-saved-response';
import { PageBlocks } from './page-blocks';
import { pageBlocks, pageOutline, splitPage } from './page-format';
import './pages.css';

/** A page from a saved reply, opened full size for reading. It reads through the
 * reply's own saved-response lifecycle, so access, versions and revocation match. */
export function PageReader({ id, source, close }: { id: string; source: ResultSource; close: () => void }) {
  const { data, latest, error, busy, update: reload } = useSavedResponse(id, source);
  const [status, setStatus] = useState('');
  const article = useRef<HTMLElement>(null);
  const page = useMemo(() => data ? splitPage(data.body).page : undefined, [data]);
  const blocks = useMemo(() => page ? pageBlocks(page.body) : [], [page]);
  const outline = useMemo(() => page ? pageOutline(page.body) : [], [page]);
  const newer = data && latest && (latest.version !== data.version || latest.state !== data.state);
  const update = () => { reload(); setStatus(''); article.current?.scrollTo?.({ top: 0 }); };
  const go = (index: number) => article.current?.querySelectorAll('.page-text h2')[index]?.scrollIntoView?.({ block: 'start' });
  const copy = () => { if (page) void navigator.clipboard.writeText(`# ${page.title}\n\n${page.body}`)
    .then(() => setStatus('Page copied.')).catch(() => setStatus('Copy failed. Select and copy the page text.')); };
  const state = !data ? error ? 'Page unavailable' : 'Loading page…' : data.state === 'draft' || (page && !page.complete) ? 'Saved draft · still being written'
    : data.state === 'interrupted' ? 'Saved partial page · work interrupted' : 'Saved page';
  return <Dialog title={page?.title ?? 'Page'} close={close} status={status}>
    <div className="page-reader">
      <div className="result-toolbar"><span>{state}</span>
        <Button disabled={!page} onClick={copy}>Copy</Button>
        {data?.state === 'complete' && <a className="ui-button" href={source.href(query(source.response(id), { download: true, version: data.version }))} download>Download Markdown</a>}
      </div>
      {(data?.state === 'draft' || newer) && <div className="result-update" role="status"><span>{newer ? 'A newer saved version is available.' : 'Your reading view stays unchanged.'}</span><Button disabled={!newer || busy} onClick={update}>Load latest saved page</Button></div>}
      {error && <p role="alert" className="chat-error">{error} <Button disabled={busy} onClick={update}>Retry</Button></p>}
      {!data && !error && <p role="status" className="result-note">Loading page…</p>}
      {data && !page && <p className="result-note">This reply no longer has a page.</p>}
      {page && <div className="page-layout">
        {outline.length > 1 && <nav className="page-outline" aria-label="On this page">
          <h3>On this page</h3>
          {outline.map((title, index) => <button type="button" key={index} onClick={() => go(index)}>{title}</button>)}
        </nav>}
        <article ref={article} tabIndex={0} aria-label="Page content" className="page-body">
          <h1>{page.title}</h1>
          <PageBlocks blocks={blocks} />
        </article>
      </div>}
    </div>
  </Dialog>;
}
