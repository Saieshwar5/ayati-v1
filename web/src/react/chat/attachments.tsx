import { useEffect, useRef, useState } from 'react';
import { workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import type { Attachment } from './model';
export function Attachments({ chat, value, change, disabled, pending }: { chat: string; value: Attachment[]; change: (files: Attachment[]) => void; disabled: boolean; pending?: (busy: boolean) => void }) {
  const [error, setError] = useState(''); const [uploading, setUploading] = useState(false); const [recording, setRecording] = useState(false);
  const recorder = useRef<MediaRecorder | undefined>(undefined); const stream = useRef<MediaStream | undefined>(undefined); const alive = useRef(false); const gate = useRef(false); const requestingAudio = useRef(false); const abort = useRef<AbortController | undefined>(undefined);
  const current = useRef(value); current.current = value;
  useEffect(() => { alive.current = true; return () => { alive.current = false; abort.current?.abort(); if (recorder.current?.state === 'recording') recorder.current.stop(); stream.current?.getTracks().forEach(t => t.stop()); }; }, []);
  const upload = async (file: File) => {
    if (gate.current || disabled) return;
    if (file.size > 10 * 1024 * 1024 || !file.size || current.current.length >= 8) { setError('Choose a file up to 10 MiB; at most eight attachments.'); pending?.(false); return; }
    gate.current = true; pending?.(true); setUploading(true); setError(''); abort.current = new AbortController();
    try {
      const response = await fetch(`/api/identity/workspaces/${workspaceId}/chats/${chat}/objects?name=${encodeURIComponent(file.name)}`, { method: 'POST', headers: { 'X-Ayati-Request': 'identity', 'Content-Type': file.type || 'application/octet-stream' }, body: file, signal: abort.current.signal });
      const data = await response.json(); if (!response.ok) throw new Error(data.error ?? 'Upload failed');
      if (alive.current) change([...current.current, data]);
    } catch (e) { if (alive.current) setError(String(e)); }
    finally { gate.current = false; if (alive.current) { setUploading(false); pending?.(false); } }
  };
  const record = async () => {
    if (recorder.current?.state === 'recording') { recorder.current.stop(); return; }
    if (gate.current || disabled || requestingAudio.current) return;
    requestingAudio.current = true;
    try {
      const media = await navigator.mediaDevices.getUserMedia({ audio: true });
      if (!alive.current) { media.getTracks().forEach(t => t.stop()); return; }
      stream.current = media; const chunks: Blob[] = []; let size = 0;
      const instance = new MediaRecorder(media); recorder.current = instance;
      instance.ondataavailable = event => { chunks.push(event.data); size += event.data.size; if (size > 9 * 1024 * 1024 && instance.state === 'recording') instance.stop(); };
      instance.onstop = () => { media.getTracks().forEach(t => t.stop()); if (!alive.current) return; setRecording(false); void upload(new File(chunks, 'voice-message.webm', { type: instance.mimeType })); };
      instance.start(1000); setRecording(true); pending?.(true);
    } catch (e) { if (alive.current) { setError(`Recording unavailable: ${String(e)}`); pending?.(false); } }
    finally { requestingAudio.current = false; }
  };
  return <div className="chat-attachments"><label className="chat-file-picker ui-button"><span>{uploading ? 'Uploading…' : 'Attach file'}</span><input aria-label="Attach file" type="file" disabled={disabled || uploading || recording} onChange={e => { const file = e.target.files?.[0]; if (file) void upload(file); e.target.value = ''; }} /></label>
    {typeof MediaRecorder !== 'undefined' && <Button disabled={disabled || uploading} onClick={() => void record()}>{recording ? 'Stop recording' : 'Record voice'}</Button>}
    {value.map(file => <Button key={file.id} disabled={disabled} onClick={() => change(value.filter(f => f.id !== file.id))}>Remove {file.name}</Button>)}
    {error && <p role="alert">{error}</p>}
  </div>;
}
