import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { query, type ResultSource } from './model';
import { useSavedFile } from './use-saved-file';

type TextPreview = { text: string | null; truncated: boolean };

/** A saved file that is not a picture: audio plays, text shows as plain text, and anything
 * else offers its original. Pictures open in the image viewer (image-viewer.tsx). */
export function FileViewer({ id, name, source, close }: { id: string; name: string; source: ResultSource; close: () => void }) {
  const [preview, setPreview] = useState<TextPreview>();
  const [mediaFailed, setMediaFailed] = useState(false);
  const { file, error, retry } = useSavedFile(id, source, async (opened, signal) => {
    setPreview(undefined);
    setMediaFailed(false);
    if (opened.mime.startsWith('audio/')) return;
    const text = await source.read<TextPreview>(query(source.file(id), { preview: true, version: opened.checksum }), signal);
    if (!signal.aborted) setPreview(text);
  });
  const audio = file?.mime.startsWith('audio/');
  // Without the file's details (still loading, or access was lost) nothing of it is shown.
  const text = file ? preview?.text : undefined;
  return <Dialog title={name} close={close}>
    <div className="result-file-viewer">
      <div className="result-toolbar"><span>{file ? `${Math.ceil(file.size / 1024)} KB · Original file` : 'Loading file…'}</span>
        {file && <a className="ui-button" href={source.href(query(source.file(id), { version: file.checksum }))} download={file.name}>Download original</a>}
      </div>
      {error && <p className="chat-error" role="alert">{error} <Button onClick={retry}>Retry</Button></p>}
      {file && audio && !mediaFailed && <audio className="result-audio" controls preload="metadata" src={source.href(query(source.file(id), { inline: true, version: file.checksum }))} aria-label={file.name} onError={() => setMediaFailed(true)} />}
      {file && (mediaFailed || text === null) && <p className="result-note">Preview unavailable for this file. Download the original to open it.</p>}
      {typeof text === 'string' && <><pre className="result-text-file" tabIndex={0}>{text}</pre>{preview?.truncated && <p className="result-note">Preview shortened to 64 KiB. Download the original for the complete file.</p>}</>}
    </div>
  </Dialog>;
}
