import type { Attachment } from '../chat/model';

/** Under a shared document: whether agents could read it (schema 100). Quiet when
 * they can, so only a file still being read or one agents can't read says anything.
 * Images stay quiet: most carry no text, and people can see them anyway. */
export function FileReading({ file }: { file: Attachment }) {
  if (file.mime.startsWith('image/') || file.mime.startsWith('audio/')) return null;
  if (file.reading === 'pending') return <small className="chat-file-reading" role="status">Reading for agents…</small>;
  if (file.reading === 'unsupported' || file.reading === 'failed') return <small className="chat-file-reading">Agents can't read this file</small>;
  return null;
}
