import { useState } from 'react';
import { isViewerImage } from '../results/model';
import { FileName, FileTile, previewUrl } from '../results/files';
import { fileSize } from '../results/file-kind';
import type { Attachment } from './model';

/** A file waiting in the message box: a picture as a small preview, anything else as its
 * type tile, name and size. Each has its own × to take it out before sending. */
export function AttachedFile({ file, disabled, remove }: { file: Attachment; disabled: boolean; remove: () => void }) {
  const [missing, setMissing] = useState(false);
  const picture = isViewerImage(file.mime) && !missing;
  return <span className={`composer-file${picture ? ' is-picture' : ''}`}>
    {picture ? <img src={previewUrl(file)} alt={file.name} title={file.name} onError={() => setMissing(true)} />
      : <><FileTile file={file} /><span className="chat-file-text"><FileName name={file.name} /><small>{fileSize(file.size)}</small></span></>}
    <button type="button" className="composer-file-remove" aria-label={`Remove ${file.name}`} title="Remove" disabled={disabled} onClick={remove}>×</button>
  </span>;
}

/** The file being stored, in the place it will take once it is attached. */
export function UploadingFile({ name }: { name: string }) {
  return <span className="composer-file is-uploading" role="status">
    <span className="composer-file-spinner" aria-hidden="true" />
    <span className="chat-file-text"><FileName name={name} /><small>Uploading…</small></span>
  </span>;
}
