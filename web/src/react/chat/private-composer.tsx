import { StrictMode, useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import type { ViewLifetime } from '../../view-lifetime';
import { storageScope } from '../../workspace-context';
import { PendingSubmission } from '../../submission';
import type { Artifact } from '../../types';
import { Button } from '../ui';
import { Composer } from './composer';
import { VoiceRecorder } from './voice-recorder';

type Options = { ready: boolean; uploads: boolean; name: string; placeholder: string; task?: string };
type Actions = { transform: (text: string) => string; refresh: () => Promise<void>; stop: (id: string) => Promise<void> };
type FileItem = { id: string; name: string };
function PrivateComposer({ view, options, actions }: { view: ViewLifetime; options: Options; actions: Actions }) {
  const draftKey = storageScope(`ayati.draft.${view.agent}`), filesKey = storageScope(`ayati.attachments.${view.agent}`);
  const storage = (() => { try { return window.sessionStorage; } catch { return undefined; } })();
  const submission = useRef(new PendingSubmission(storage, storageScope(view.agent)));
  const [draft, setDraft] = useState(() => { try { return storage?.getItem(draftKey) ?? submission.current.draft(); } catch { return submission.current.draft(); } });
  const [files, setFiles] = useState<FileItem[]>(() => { try { const saved = JSON.parse(storage?.getItem(filesKey) ?? '[]'); return Array.isArray(saved) ? saved.filter(f => typeof f.id === 'string' && typeof f.name === 'string').slice(0,8) : []; } catch { return []; } });
  const [sending, setSending] = useState(false), [uploading, setUploading] = useState(false), [recording, setRecording] = useState(false), [error, setError] = useState('');
  const gate = useRef(false), uploadGate = useRef(false), alive = useRef(false), picker = useRef<HTMLInputElement>(null);
  const latestFiles = useRef(files); latestFiles.current = files;
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const change = (text: string) => { setDraft(text); try { storage?.setItem(draftKey,text); } catch { /* Optional draft storage. */ } };
  const saveFiles = (next: FileItem[]) => { setFiles(next); try { storage?.setItem(filesKey,JSON.stringify(next)); } catch { /* Optional draft storage. */ } };
  const upload = async (file: File) => {
    if (uploadGate.current || gate.current || !options.uploads) throw new Error('Upload unavailable');
    if (!file.size || file.size > 25*1024*1024 || latestFiles.current.length >= 8) { setError('Choose a file up to 25 MiB; at most eight attachments.'); throw new Error('Invalid file'); }
    uploadGate.current = true; setUploading(true); setError('');
    const body = new FormData(); body.append('file',file);
    try {
      const result = await view.api<{ artifact: Artifact }>('/files', { method:'POST',body });
      if (alive.current) saveFiles([...latestFiles.current,{ id:result.artifact.id,name:file.name }]);
    } catch(e) { if (alive.current) setError(e instanceof Error ? e.message : 'Upload failed'); throw e; }
    finally { uploadGate.current = false; if (alive.current) setUploading(false); }
  };
  const send = async () => {
    if (gate.current || uploadGate.current || recording || !options.ready || (!draft.trim() && !files.length)) return;
    gate.current = true; setSending(true); setError('');
    const request = submission.current.prepare(actions.transform(draft.trim()),files.map(f => f.id));
    try {
      await view.post('/messages',request); submission.current.acknowledge(request.request_id);
      if (!alive.current) return;
      change(''); saveFiles([]); await actions.refresh();
    } catch(e) { if (alive.current) setError(e instanceof TypeError ? 'Connection lost. Your draft is kept; retry the same message safely.' : (e as Error).message); }
    finally { gate.current = false; if (alive.current) setSending(false); }
  };
  return <><Composer maxLength={16000} value={draft} change={change} send={() => void send()} disabled={sending} label={`Message ${options.name}`} placeholder={options.placeholder}
    sendDisabled={!options.ready || uploading || recording || (!draft.trim() && !files.length)}
    attachments={files.map(file => <Button key={file.id} disabled={sending || uploading} aria-label={`Remove ${file.name}`} onClick={() => saveFiles(files.filter(f => f.id !== file.id))}>{file.name} ×</Button>)}
    trailing={options.task && <Button onClick={() => void actions.stop(options.task!)}>Stop</Button>}
    media={options.uploads && <>
      <input hidden ref={picker} type="file" aria-label="Choose attachment" onChange={event => { const file = event.target.files?.[0]; if (file) void upload(file).catch(() => {}); event.target.value = ''; }} />
      <Button aria-label="Attach file" title="Attach file" disabled={sending || uploading || recording} onClick={() => picker.current?.click()}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="m8 12 7-7a4 4 0 0 1 6 6l-10 10a6 6 0 0 1-8-8L13 3m-7 12 9-9a2 2 0 0 1 3 3l-9 9"/></svg></Button>
      <VoiceRecorder disabled={sending || uploading} attach={upload} pending={setRecording} />
      {uploading && <small role="status">Uploading…</small>}
    </>} />{error && <p className="chat-error" role="alert">{error}</p>}</>;
}
/** Explicit React-owned island inside the documented compatibility subtree. */
export function mountPrivateComposer(host: HTMLElement, view: ViewLifetime, actions: Actions) {
  const root = createRoot(host);
  let options: Options = { ready:false, uploads:false, name:'Cuckoo', placeholder:'Message…' };
  const update = (next: Partial<Options>) => { options = { ...options,...next }; if (view.active) root.render(<StrictMode><PrivateComposer view={view} options={options} actions={actions} /></StrictMode>); };
  update({});
  view.cleanup(() => queueMicrotask(() => root.unmount()));
  return { update, focus: () => host.querySelector('textarea')?.focus(), setPlaceholder: (placeholder: string) => update({ placeholder }) };
}
