import { useEffect, useState } from 'react';
import { Button, Dialog } from '../ui';
import { errorMessage } from '../requests';
import { denied, query, type ResultSource, type SavedFile } from './model';
export function FileViewer({ id, name, source, close }: { id: string; name: string; source: ResultSource; close: () => void }) {
  const [file, setFile] = useState<SavedFile>();
  const [preview, setPreview] = useState<{ text: string | null; truncated: boolean }>();
  const [error, setError] = useState('');
  const [imageFailed, setImageFailed] = useState(false);
  const [zoom, setZoom] = useState<number>();
  const [naturalWidth, setNaturalWidth] = useState(0);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    const abort = new AbortController(); let checking = false;
    setFile(undefined); setPreview(undefined); setError(''); setImageFailed(false);
    setNaturalWidth(0); setZoom(undefined);
    void source.read<SavedFile>(query(source.file(id), { metadata: true }), abort.signal).then(async data => {
      if (abort.signal.aborted) return;
      setFile(data);
      if (!['image/png', 'image/jpeg'].includes(data.mime)) {
        const text = await source.read<{ text: string | null; truncated: boolean }>(query(source.file(id), { preview: true, version: data.checksum }), abort.signal);
        if (!abort.signal.aborted) setPreview(text);
      }
    }).catch(failure => { if (!abort.signal.aborted) { if (denied(failure)) setFile(undefined); setError(errorMessage(failure)); } });
    const check = async () => {
      if (checking || document.hidden || abort.signal.aborted) return;
      checking = true;
      try { await source.read(query(source.file(id), { access: true }), abort.signal); }
      catch (failure) { if (!abort.signal.aborted) { if (denied(failure)) { setFile(undefined); setPreview(undefined); abort.abort(); } setError(errorMessage(failure)); } }
      finally { checking = false; }
    };
    const timer = window.setInterval(() => void check(), 5000);
    const unsubscribe = source.subscribe?.(() => void check());
    return () => { abort.abort(); window.clearInterval(timer); unsubscribe?.(); };
  }, [id, source, retry]);
  const image = file && ['image/png', 'image/jpeg'].includes(file.mime);
  return <Dialog title={name} close={close}>
    <div className="result-file-viewer">
      <div className="result-toolbar"><span>{file ? `${Math.ceil(file.size / 1024)} KB · Original file` : 'Loading file…'}</span>
        {image && <><Button onClick={() => setZoom(undefined)}>Fit</Button><Button aria-label="Zoom out" onClick={() => setZoom(value => Math.max(25, (value ?? 100) - 25))}>−</Button><output aria-label="Zoom">{zoom ? `${zoom}%` : 'Fit'}</output><Button aria-label="Zoom in" onClick={() => setZoom(value => Math.min(400, (value ?? 100) + 25))}>+</Button></>}
        {file && <a className="ui-button" href={source.href(query(source.file(id), { version: file.checksum }))} download={file.name}>Download original</a>}
      </div>
      {error && <p className="chat-error" role="alert">{error} <Button onClick={() => setRetry(value => value + 1)}>Retry</Button></p>}
      {image && !imageFailed && <div className={`result-image-stage${zoom ? ' result-image-zoomed' : ''}`} tabIndex={0} aria-label="Image preview"><img src={source.href(query(source.file(id), { inline: true, version: file.checksum }))} alt={file.name} style={zoom && naturalWidth ? { width: `${naturalWidth * zoom / 100}px` } : undefined} onLoad={event => setNaturalWidth(event.currentTarget.naturalWidth)} onError={() => setImageFailed(true)} /></div>}
      {(imageFailed || preview?.text === null) && <p className="result-note">Preview unavailable for this file. Download the original to open it.</p>}
      {preview?.text !== null && preview?.text !== undefined && <><pre className="result-text-file" tabIndex={0}>{preview.text}</pre>{preview.truncated && <p className="result-note">Preview shortened to 64 KiB. Download the original for the complete file.</p>}</>}
    </div>
  </Dialog>;
}
