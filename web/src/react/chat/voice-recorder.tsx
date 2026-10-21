import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
export function VoiceRecorder({ disabled, attach, pending }: { disabled: boolean; attach: (file: File) => Promise<void>; pending: (value: boolean) => void }) {
  const [state, setState] = useState<'idle'|'requesting'|'recording'|'uploading'|'preview'>('idle');
  const [seconds, setSeconds] = useState(0);
  const [preview, setPreview] = useState<{ file: File; url: string }>();
  const [error, setError] = useState('');
  const generation = useRef(0);
  const uploadGate = useRef(false);
  const latest = useRef({ attach, pending }); latest.current = { attach, pending };
  const recorder = useRef<MediaRecorder | undefined>(undefined);
  const stream = useRef<MediaStream | undefined>(undefined);
  const alive = useRef(false), cancelled = useRef(false), gate = useRef(false);
  useEffect(() => { alive.current = true; return () => {
    alive.current = false; cancelled.current = true; generation.current++;
    if (recorder.current?.state === 'recording') recorder.current.stop();
    stream.current?.getTracks().forEach(track => track.stop());
  }; }, []);
  useEffect(() => { if (state !== 'recording') return; const timer = window.setInterval(() => setSeconds(value => value + 1), 1000); return () => window.clearInterval(timer); }, [state]);
  useEffect(() => () => { if (preview) URL.revokeObjectURL(preview.url); }, [preview]);
  const cancel = () => {
    cancelled.current = true; generation.current++;
    if (recorder.current?.state === 'recording') recorder.current.stop();
    stream.current?.getTracks().forEach(track => track.stop());
    setPreview(undefined); setError(''); setState('idle'); pending(false); gate.current = false;
  };
  const start = async () => {
    if (gate.current || disabled) return;
    const version = ++generation.current;
    gate.current = true; cancelled.current = false; setError(''); setState('requesting'); pending(true);
    try {
      const media = await navigator.mediaDevices.getUserMedia({ audio: true });
      if (!alive.current || cancelled.current || version !== generation.current) { media.getTracks().forEach(track => track.stop()); return; }
      stream.current = media;
      const instance = new MediaRecorder(media), chunks: Blob[] = []; let size = 0;
      recorder.current = instance;
      instance.ondataavailable = event => { chunks.push(event.data); size += event.data.size; if (size > 9 * 1024 * 1024 && instance.state === 'recording') instance.stop(); };
      instance.onerror = () => { if (alive.current && version === generation.current) { cancel(); setError('Recording failed. Please try again.'); } };
      instance.onstop = () => {
        media.getTracks().forEach(track => track.stop());
        if (!alive.current || cancelled.current || version !== generation.current) return;
        const mime = instance.mimeType || 'audio/webm';
        const file = new File(chunks, `voice-message.${mime.includes('mp4') ? 'm4a' : 'webm'}`, { type: mime });
        setPreview({ file, url: URL.createObjectURL(file) });
        void accept(file, version);
      };
      instance.start(1000); setSeconds(0); setState('recording');
    } catch { if (alive.current && version === generation.current) { cancel(); setError('Microphone unavailable. Check permission and try again.'); } }
  };
  const accept = async (file: File, version = generation.current) => {
    if (uploadGate.current) return;
    uploadGate.current = true; setState('uploading'); setError('');
    try {
      await latest.current.attach(file);
      if (alive.current && version === generation.current) {
        setPreview(undefined); setState('idle'); gate.current = false;
        latest.current.pending(false);
      }
    } catch {
      if (alive.current && version === generation.current) {
        setState('preview'); setError('Voice upload failed. Your recording is kept here; retry or discard it.');
      }
    } finally { uploadGate.current = false; }
  };
  if (typeof MediaRecorder === 'undefined') return null;
  return <div className={`composer-voice${state !== 'idle' ? ' is-active' : ''}`}>
    {state === 'idle' ? <Button aria-label="Record voice" title="Record voice" disabled={disabled} onClick={() => void start()}><svg viewBox="0 0 24 24" aria-hidden="true"><rect x="9" y="3" width="6" height="12" rx="3"/><path d="M5 10v2a7 7 0 0 0 14 0v-2M12 19v3M8 22h8"/></svg></Button> : <>
      <span role="status">{state === 'recording' ? `Recording ${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2,'0')}` : state === 'requesting' ? 'Waiting for microphone…' : state === 'uploading' ? 'Attaching voice…' : 'Recording kept'}</span>
      {state === 'recording' && <Button onClick={() => { if (recorder.current?.state === 'recording') recorder.current.stop(); }}>Stop recording</Button>}
      {state === 'preview' && preview && <><audio controls src={preview.url} aria-label="Recording preview" /><Button disabled={disabled} onClick={() => void accept(preview.file)}>Retry voice upload</Button></>}
      <Button disabled={state === 'uploading'} onClick={cancel}>Cancel recording</Button>
    </>}
    {error && <span role="alert">{error}</span>}
  </div>;
}
