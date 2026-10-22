import { useEffect, useRef, useState } from 'react';
import { getDocument, GlobalWorkerOptions, type PDFDocumentProxy } from 'pdfjs-dist';
import workerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
import { Button } from '../ui';
import { downloadUrl } from './model';
GlobalWorkerOptions.workerSrc = workerUrl;
export function PdfPreview({ id }: { id: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [pdf, setPdf] = useState<PDFDocumentProxy>(),
    [page, setPage] = useState(1),
    [error, setError] = useState('');
  useEffect(() => {
    let alive = true;
    const task = getDocument({
      url: downloadUrl(id, true),
      withCredentials: true,
      enableXfa: false,
      disableAutoFetch: true,
      disableStream: true,
      useSystemFonts: true,
    });
    void task.promise
      .then((document) => {
        if (alive) setPdf(document);
      })
      .catch(() => {
        if (alive) setError('This PDF could not be previewed. Download the original to open it.');
      });
    return () => {
      alive = false;
      void task.destroy();
    };
  }, [id]);
  useEffect(() => {
    if (!pdf || !canvas.current) return;
    let alive = true,
      cancel = () => {};
    void pdf
      .getPage(page)
      .then((source) => {
        if (!alive || !canvas.current) return;
        const natural = source.getViewport({ scale: 1 });
        const viewport = source.getViewport({
          scale: Math.min(1.5, 1200 / natural.width, 1600 / natural.height),
        });
        canvas.current.width = viewport.width;
        canvas.current.height = viewport.height;
        const rendering = source.render({ canvas: canvas.current, viewport });
        cancel = () => rendering.cancel();
        return rendering.promise;
      })
      .catch((reason) => {
        if (alive && reason?.name !== 'RenderingCancelledException')
          setError('This page could not be previewed.');
      });
    return () => {
      alive = false;
      cancel();
    };
  }, [pdf, page]);
  return (
    <div className="library-pdf-viewer">
      {error ? (
        <p role="status">{error}</p>
      ) : (
        <>
          {!pdf && <p role="status">Opening PDF…</p>}
          {pdf && (
            <div className="library-pagination">
              <Button disabled={page <= 1} onClick={() => setPage((n) => n - 1)}>
                Previous page
              </Button>
              <span>
                Page {page} of {pdf.numPages}
              </span>
              <Button disabled={page >= pdf.numPages} onClick={() => setPage((n) => n + 1)}>
                Next page
              </Button>
            </div>
          )}
          <canvas
            ref={canvas}
            role="img"
            aria-label={`PDF page ${page}. Download the original for its accessible text.`}
          />
        </>
      )}
    </div>
  );
}
