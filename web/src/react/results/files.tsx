import { useState } from 'react';
import type { Attachment } from '../chat/model';
import { workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useResults } from './provider';
import { isViewerImage } from './model';
import { AudioTranscript } from './transcript';
import { isVoiceNote, VoicePlayer } from './voice-player';
export function MessageFiles({ files }: { files: Attachment[] }) {
  const open = useResults();
  const [more, setMore] = useState(false);
  return <div className="result-files">{(more ? files : files.slice(0, 3)).map(file => {
    const url = `/api/identity/workspaces/${workspaceId}/objects/${encodeURIComponent(file.id)}`;
    const audio = file.mime.startsWith('audio/');
    const image = isViewerImage(file.mime);
    const view = open && (() => open({ kind: image ? 'image' : 'file', id: file.id, name: file.name }));
    return <div className="chat-file" key={file.id}>
      {image && view && <button type="button" className="result-thumbnail" aria-label={`Open ${file.name}`} onClick={view}><img loading="lazy" src={`${url}?thumbnail=true&version=${encodeURIComponent(file.checksum)}`} alt={file.name} onError={event => { event.currentTarget.hidden = true; }} /></button>}
      {audio && <><VoicePlayer file={file} src={`${url}?inline=true`} /><AudioTranscript file={file} /></>}
      {/* A recorded voice message is just its player and transcript (D245). */}
      {!(audio && isVoiceNote(file)) && <div>{view ? <Button onClick={view}>{file.name}</Button> : <a href={url} download={file.name}>{file.name}</a>}<small> · {Math.ceil(file.size / 1024)} KB</small><a href={`${url}?version=${encodeURIComponent(file.checksum)}`} download={file.name} aria-label={`Download ${file.name}`}>↓</a></div>}
    </div>;
  })}{files.length > 3 && <Button onClick={() => setMore(value => !value)}>{more ? 'Show fewer files' : `Show ${files.length - 3} more files`}</Button>}</div>;
}
