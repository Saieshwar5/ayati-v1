import { useEffect, useRef, useState } from 'react';
import { workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { Composer, type ComposerProps } from './composer';
import { VoiceRecorder } from './voice-recorder';
import type { Attachment } from './model';
export function ChatComposer({ chat, files, setFiles, ...props }: ComposerProps & { chat: string; files: Attachment[]; setFiles: (files: Attachment[]) => void }) {
  const [error, setError] = useState(''), [uploading, setUploading] = useState(false), [recording, setRecording] = useState(false);
  const alive = useRef(false), gate = useRef(false), current = useRef(files);
  const picker = useRef<HTMLInputElement>(null), abort = useRef<AbortController | undefined>(undefined);
  current.current = files;
  useEffect(() => { alive.current = true; return () => { alive.current = false; abort.current?.abort(); }; }, []);
  const upload = async (file: File) => {
    if (gate.current || props.disabled) throw new Error('Upload in progress');
    if (!file.size || file.size > 10*1024*1024 || current.current.length >= 8) { setError('Choose a file up to 10 MiB; at most eight attachments.'); throw new Error('Invalid attachment'); }
    gate.current = true; setUploading(true); setError(''); abort.current = new AbortController();
    try {
      const response = await fetch(`/api/identity/workspaces/${workspaceId}/chats/${chat}/objects?name=${encodeURIComponent(file.name)}`, { method: 'POST', headers: { 'X-Ayati-Request': 'identity', 'Content-Type': file.type || 'application/octet-stream' }, body: file, signal: abort.current.signal });
      const data = await response.json(); if (!response.ok) throw new Error(data.error ?? 'Upload failed');
      if (alive.current) setFiles([...current.current, data]);
    } catch (e) { if (alive.current) setError(e instanceof Error ? e.message : 'Upload failed'); throw e; }
    finally { gate.current = false; if (alive.current) setUploading(false); }
  };
  return <><Composer {...props} sendDisabled={props.sendDisabled || uploading || recording} attachments={files.map(file => <Button key={file.id} aria-label={`Remove ${file.name}`} disabled={props.disabled || uploading} onClick={() => setFiles(files.filter(f => f.id !== file.id))}>{file.name} ×</Button>)} media={<>
    <input ref={picker} type="file" hidden aria-label="Choose attachment" onChange={event => { const file = event.target.files?.[0]; if (file) void upload(file).catch(() => {}); event.target.value = ''; }} />
    <Button aria-label="Attach file" title="Attach file" disabled={props.disabled || uploading || recording} onClick={() => picker.current?.click()}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m8 12 7-7a4 4 0 0 1 6 6l-10 10a6 6 0 0 1-8-8L13 3m-7 12 9-9a2 2 0 0 1 3 3l-9 9"/></svg></Button>
    <VoiceRecorder disabled={!!props.disabled || uploading} attach={upload} pending={setRecording} />
    {uploading && <small role="status">Uploading…</small>}
  </>} />{error && <p className="chat-error" role="alert">{error}</p>}</>;
}
