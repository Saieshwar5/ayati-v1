import { useState } from 'react';
import type { Attachment } from '../chat/model';
import { workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useResults } from './provider';
export function MessageFiles({ files }: { files: Attachment[] }) {
  const open = useResults();
  const [more, setMore] = useState(false);
  return <div className="result-files">{(more ? files : files.slice(0, 3)).map(file => {
    const url = `/api/identity/workspaces/${workspaceId}/objects/${encodeURIComponent(file.id)}`;
    return <div className="chat-file" key={file.id}>
      {['image/png', 'image/jpeg'].includes(file.mime) && open && <button type="button" className="result-thumbnail" aria-label={`Open ${file.name}`} onClick={() => open({ kind: 'file', id: file.id, name: file.name })}><img loading="lazy" src={`${url}?thumbnail=true&version=${encodeURIComponent(file.checksum)}`} alt={file.name} onError={event => { event.currentTarget.hidden = true; }} /></button>}
      {file.mime.startsWith('audio/') && <audio controls preload="none" src={`${url}?inline=true`} aria-label={file.name} />}
      <div>{open ? <Button onClick={() => open({ kind: 'file', id: file.id, name: file.name })}>{file.name}</Button> : <a href={url} download={file.name}>{file.name}</a>}<small> · {Math.ceil(file.size / 1024)} KB</small><a href={`${url}?version=${encodeURIComponent(file.checksum)}`} download={file.name} aria-label={`Download ${file.name}`}>↓</a></div>
    </div>;
  })}{files.length > 3 && <Button onClick={() => setMore(value => !value)}>{more ? 'Show fewer files' : `Show ${files.length - 3} more files`}</Button>}</div>;
}
