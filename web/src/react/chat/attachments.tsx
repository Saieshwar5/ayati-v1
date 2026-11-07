import { useEffect, useRef, useState } from 'react';
import { workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { Composer, type ComposerProps } from './composer';
import { VoiceButton, VoiceDraft, VoiceRow } from './voice-controls';
import { useVoiceRecorder } from './voice-recorder';
import type { Attachment } from './model';

const MAX_BYTES = 10 * 1024 * 1024, MAX_FILES = 8;

/** Stores one file in the conversation; the message that shares it is sent separately. */
async function uploadChatFile(chat: string, file: File, signal: AbortSignal): Promise<Attachment> {
  const response = await fetch(`/api/identity/workspaces/${workspaceId}/chats/${chat}/objects?name=${encodeURIComponent(file.name)}`, { method: 'POST', headers: { 'X-Ayati-Request': 'identity', 'Content-Type': file.type || 'application/octet-stream' }, body: file, signal });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error ?? `Upload failed (${response.status}).`);
  return data;
}

export type ChatComposerProps = Omit<ComposerProps, 'send'> & {
  chat: string; files: Attachment[]; setFiles: (files: Attachment[]) => void;
  /** Sends the draft with its files, plus a just-recorded voice message; true once accepted. */
  send: (voice?: Attachment) => Promise<boolean>;
};

export function ChatComposer({ chat, files, setFiles, send, ...props }: ChatComposerProps) {
  const [error, setError] = useState(''), [uploading, setUploading] = useState(false);
  const alive = useRef(false), gate = useRef(false), current = useRef(files);
  const picker = useRef<HTMLInputElement>(null), abort = useRef<AbortController | undefined>(undefined);
  current.current = files;
  useEffect(() => { alive.current = true; return () => { alive.current = false; abort.current?.abort(); }; }, []);
  const upload = async (file: File) => {
    if (gate.current || props.disabled) return;
    if (!file.size || file.size > MAX_BYTES || current.current.length >= MAX_FILES) { setError('Choose a file up to 10 MiB; at most eight attachments.'); return; }
    gate.current = true; setUploading(true); setError(''); abort.current = new AbortController();
    try {
      const data = await uploadChatFile(chat, file, abort.current.signal);
      if (alive.current) setFiles([...current.current, data]);
    } catch (e) { if (alive.current) setError(e instanceof Error ? e.message : 'Upload failed'); }
    finally { gate.current = false; if (alive.current) setUploading(false); }
  };
  const voice = useVoiceRecorder({
    disabled: !!props.disabled || uploading, send,
    upload: async (file, signal) => {
      if (file.size > MAX_BYTES) throw new Error('The recording is over the 10 MiB limit.');
      if (current.current.length >= MAX_FILES) throw new Error('A message can share at most eight files. Remove one, then retry.');
      return uploadChatFile(chat, file, signal);
    },
  });
  const { phase } = voice;
  const showsRow = phase.name === 'requesting' || phase.name === 'recording' || phase.name === 'sending';
  const kept = phase.name === 'ready' || phase.name === 'failed' ? phase : undefined;
  // While a recording exists, Send (and Enter) sends it, with any typed text.
  const sendsVoice = phase.name === 'recording' || Boolean(kept);
  return <><Composer {...props}
    send={sendsVoice ? () => void voice.submit() : () => void send()}
    sendDisabled={sendsVoice ? uploading : showsRow || props.sendDisabled || uploading}
    voice={showsRow ? { row: <VoiceRow voice={voice} />, cancel: phase.name === 'sending' ? undefined : voice.cancel } : undefined}
    attachments={<>
      {files.map(file => <Button key={file.id} aria-label={`Remove ${file.name}`} disabled={props.disabled || uploading} onClick={() => setFiles(files.filter(f => f.id !== file.id))}>{file.name} ×</Button>)}
      {kept && <VoiceDraft phase={kept} disabled={!!props.disabled || uploading} retry={() => void voice.submit()} remove={voice.cancel} />}
    </>}
    trailing={kept ? undefined : <VoiceButton voice={voice} disabled={!!props.disabled || uploading} />}
    media={<>
      <input ref={picker} type="file" hidden aria-label="Choose attachment" onChange={event => { const file = event.target.files?.[0]; if (file) void upload(file); event.target.value = ''; }} />
      <Button aria-label="Attach file" title="Attach file" disabled={props.disabled || uploading} onClick={() => picker.current?.click()}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m8 12 7-7a4 4 0 0 1 6 6l-10 10a6 6 0 0 1-8-8L13 3m-7 12 9-9a2 2 0 0 1 3 3l-9 9"/></svg></Button>
      {uploading && <small role="status">Uploading…</small>}
    </>} />
    {voice.problem && <p className="chat-error" role="alert">{voice.problem}</p>}
    {error && <p className="chat-error" role="alert">{error}</p>}</>;
}
