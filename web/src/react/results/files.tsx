import { useState, type SyntheticEvent } from 'react';
import type { Attachment } from '../chat/model';
import { workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useResults } from './provider';
import { isViewerImage } from './model';
import { AudioTranscript } from './transcript';
import { isVoiceNote, VoicePlayer } from './voice-player';
import { fileKind, fileSize, splitName } from './file-kind';
import './message-files.css';

/** Where a chat file is read from; `version` ties a request to the bytes that were shared. */
export const objectUrl = (file: Attachment) => `/api/identity/workspaces/${workspaceId}/objects/${encodeURIComponent(file.id)}`;
const versioned = (file: Attachment) => `${objectUrl(file)}?version=${encodeURIComponent(file.checksum)}`;
export const previewUrl = (file: Attachment) => `${objectUrl(file)}?thumbnail=true&version=${encodeURIComponent(file.checksum)}`;

/** How many pictures a message shows before the last tile reads "+3". */
const SHOWN_PICTURES = 4;
/** How many rows (files, voice) a message shows before "Show 2 more files". */
const SHOWN_ROWS = 3;

/** How far a preview may be enlarged to fill its tile before it is shown at its own size. */
const ENLARGE_LIMIT = 2;

const downloadIcon = <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 4v11m0 0-4.5-4.5M12 15l4.5-4.5M5 20h14" /></svg>;

/** The coloured square that says what a file is before its name is read. */
export function FileTile({ file }: { file: Pick<Attachment, 'name' | 'mime'> }) {
  const kind = fileKind(file.name, file.mime);
  return <span className={`chat-file-tile${kind.tone ? ` is-${kind.tone}` : ''}`} aria-hidden="true">{kind.tile}</span>;
}

/** A file's name on one line; when it does not fit, its middle goes and its ending stays. */
export function FileName({ name }: { name: string }) {
  const { head, tail } = splitName(name);
  return <span className="chat-file-name"><span>{head}</span>{tail && <span>{tail}</span>}</span>;
}

/** A preview drawn to fill its tile. One far too small for that (an icon, a thin strip) is
 * shown whole at its own size instead, and one that cannot be drawn shows its name. */
function Preview({ file }: { file: Attachment }) {
  const [missing, setMissing] = useState(false);
  const [small, setSmall] = useState(false);
  if (missing) return <span className="chat-picture-missing">{file.name}</span>;
  const loaded = (event: SyntheticEvent<HTMLImageElement>) => {
    const img = event.currentTarget;
    setSmall(img.naturalWidth > 0 && (img.clientWidth > img.naturalWidth * ENLARGE_LIMIT || img.clientHeight > img.naturalHeight * ENLARGE_LIMIT));
  };
  return <img className={small ? 'is-small' : undefined} loading="lazy" decoding="async" src={previewUrl(file)} alt="" onLoad={loaded} onError={() => setMissing(true)} />;
}

/** The pictures of a message as pictures: one large, or up to four in a grid with the rest
 * counted on the last tile. Each opens the image viewer, which has its name, size and download.
 * Shows nothing where no viewer is available; `MessageFiles` then lists them as files. */
export function MessagePictures({ files }: { files: Attachment[] }) {
  const open = useResults();
  const pictures = open ? files.filter(file => isViewerImage(file.mime)) : [];
  if (!open || !pictures.length) return null;
  const shown = pictures.slice(0, SHOWN_PICTURES), extra = pictures.length - shown.length;
  return <div className={`chat-pictures chat-pictures-${shown.length}`}>
    {shown.map((file, index) => {
      const more = extra > 0 && index === shown.length - 1;
      return <button type="button" key={file.id} className="chat-picture" onClick={() => open({ kind: 'image', id: file.id, name: file.name })}
        aria-label={more ? `Open ${file.name} and ${extra} more ${extra === 1 ? 'picture' : 'pictures'}` : `Open ${file.name}`}>
        <Preview file={file} />
        {more && <span className="chat-picture-more" aria-hidden="true">+{extra}</span>}
      </button>;
    })}
  </div>;
}

/** One file: its type tile, name, type and size. The row opens it; the arrow downloads it. */
function FileRow({ file, view }: { file: Attachment; view?: () => void }) {
  const about = <><FileTile file={file} /><span className="chat-file-text"><FileName name={file.name} /><small>{fileKind(file.name, file.mime).label} · {fileSize(file.size)}</small></span></>;
  if (!view) return <div className="chat-file-row">
    <a className="chat-file-open" href={versioned(file)} download={file.name} aria-label={`Download ${file.name}`}>{about}<span className="chat-file-download is-shown">{downloadIcon}</span></a>
  </div>;
  return <div className="chat-file-row">
    <button type="button" className="chat-file-open" onClick={view}>{about}</button>
    <a className="chat-file-download" href={versioned(file)} download={file.name} aria-label={`Download ${file.name}`} title="Download">{downloadIcon}</a>
  </div>;
}

/** What a message shares besides its pictures: voice messages as players, uploaded
 * recordings as a player with their name under it, and every other file as a row. */
export function MessageFiles({ files }: { files: Attachment[] }) {
  const open = useResults();
  const [more, setMore] = useState(false);
  const rows = open ? files.filter(file => !isViewerImage(file.mime)) : files;
  if (!rows.length) return null;
  return <div className="result-files">{(more ? rows : rows.slice(0, SHOWN_ROWS)).map(file => {
    if (!file.mime.startsWith('audio/')) return <FileRow key={file.id} file={file} view={open && (() => open({ kind: 'file', id: file.id, name: file.name }))} />;
    return <div className="chat-file" key={file.id}>
      <VoicePlayer file={file} src={`${objectUrl(file)}?inline=true`} /><AudioTranscript file={file} />
      {/* A recorded voice message is just its player and transcript (D245). */}
      {!isVoiceNote(file) && <div className="chat-file-caption"><FileName name={file.name} /><small>{fileSize(file.size)}</small>
        <a className="chat-file-download is-shown" href={versioned(file)} download={file.name} aria-label={`Download ${file.name}`} title="Download">{downloadIcon}</a></div>}
    </div>;
  })}{rows.length > SHOWN_ROWS && <Button onClick={() => setMore(value => !value)}>{more ? 'Show fewer files' : `Show ${rows.length - SHOWN_ROWS} more files`}</Button>}</div>;
}
