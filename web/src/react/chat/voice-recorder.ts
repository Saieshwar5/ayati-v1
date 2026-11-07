import { useEffect, useRef, useState } from 'react';
import { errorMessage } from '../requests';
import type { Attachment } from './model';

/** D245: recordings stop at five minutes, which the transcription worker accepts. */
export const VOICE_LIMIT_SECONDS = 300;
/** Leaves room for the last one-second chunk under the 10 MiB upload limit. */
const SIZE_LIMIT = 9 * 1024 * 1024;
// Ask for a format other browsers can play; Safari may only offer MP4.
const recordingTypes = ['audio/webm;codecs=opus', 'audio/ogg;codecs=opus', 'audio/mp4'];
const recordingExtension = (mime: string) => mime.includes('mp4') ? 'm4a' : mime.includes('ogg') ? 'ogg' : 'webm';

/** `ready`: stopped by a limit or by the browser, kept until Send or Delete. */
export type VoicePhase =
  | { name: 'idle' }
  | { name: 'requesting' }
  | { name: 'recording' }
  | { name: 'ready'; seconds: number; reason: 'time' | 'size' | 'stopped' }
  | { name: 'sending'; seconds: number }
  | { name: 'failed'; seconds: number; error: string };

type Recording = {
  version: number; media: MediaStream; recorder: MediaRecorder;
  started: number; seconds: number; size: number; stopping: boolean;
  /** Resolves with the whole recording once the recorder has stopped. */
  stopped: Promise<File>;
  /** Kept after a successful upload, so Retry only repeats the send. */
  uploaded?: Attachment;
};

export const voiceClock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`;

/** Why this page can't record, before asking for the microphone. */
function recordingUnavailable() {
  if (window.isSecureContext === false) return 'Voice messages need a secure connection. Open Cuckoo over HTTPS to record.';
  if (!navigator.mediaDevices?.getUserMedia) return 'This browser can’t use a microphone here.';
  if (typeof MediaRecorder === 'undefined') return 'This browser can’t record voice messages. Try a current version of Chrome, Edge, Firefox or Safari.';
  return '';
}

function microphoneProblem(error: unknown) {
  const name = error && typeof error === 'object' && 'name' in error ? String(error.name) : '';
  if (['NotAllowedError', 'PermissionDeniedError', 'SecurityError'].includes(name))
    return 'Microphone access is blocked. Allow the microphone for this site in your browser’s settings, then try again.';
  if (['NotFoundError', 'DevicesNotFoundError', 'OverconstrainedError'].includes(name))
    return 'No microphone was found. Connect one and try again.';
  if (['NotReadableError', 'TrackStartError', 'AbortError'].includes(name))
    return 'The microphone couldn’t start. Another app may be using it.';
  return 'The microphone couldn’t start. Try again.';
}

const stopTracks = (media: MediaStream) => media.getTracks().forEach(track => track.stop());

function stop(recording: Recording) {
  recording.seconds = Math.min(VOICE_LIMIT_SECONDS, Math.floor((Date.now() - recording.started) / 1000));
  recording.stopping = true;
  if (recording.recorder.state !== 'inactive') recording.recorder.stop();
}

function discard(recording: Recording) {
  stop(recording);
  stopTracks(recording.media);
}

export type VoiceRecorder = ReturnType<typeof useVoiceRecorder>;

/**
 * The composer's voice message (D245): the mic starts recording, Send stops, uploads and
 * sends it in one action, and the mic again or Escape cancels it. A failed upload or send
 * keeps the recording for Retry or Delete. The recorder owns the stream and its tracks.
 */
export function useVoiceRecorder({ disabled, upload, send }: {
  disabled: boolean;
  upload: (file: File, signal: AbortSignal) => Promise<Attachment>;
  /** Resolves true once the message (with this attachment) is accepted for sending. */
  send: (voice: Attachment) => Promise<boolean>;
}) {
  const [phase, setPhase] = useState<VoicePhase>({ name: 'idle' });
  const [elapsed, setElapsed] = useState(0);
  const [problem, setProblem] = useState('');
  const [stream, setStream] = useState<MediaStream>();
  const latest = useRef({ upload, send }); latest.current = { upload, send };
  const session = useRef<Recording>(undefined);
  const generation = useRef(0), gate = useRef(false), sending = useRef(false), alive = useRef(false);
  const abort = useRef<AbortController>(undefined);
  const reset = () => { session.current = undefined; gate.current = false; setStream(undefined); setElapsed(0); setPhase({ name: 'idle' }); };

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false; generation.current++; gate.current = false; abort.current?.abort();
      const recording = session.current; session.current = undefined;
      if (recording) discard(recording);
    };
  }, []);

  useEffect(() => {
    const recording = session.current;
    if (phase.name !== 'recording' || !recording) return;
    const tick = () => {
      const ms = Date.now() - recording.started;
      setElapsed(Math.min(VOICE_LIMIT_SECONDS, Math.floor(ms / 1000)));
      // Stop and keep it: sending is always the person's choice.
      if (ms >= VOICE_LIMIT_SECONDS * 1000 && !recording.stopping) {
        stop(recording); setStream(undefined); setPhase({ name: 'ready', seconds: recording.seconds, reason: 'time' });
      }
    };
    const timer = window.setInterval(tick, 250);
    return () => window.clearInterval(timer);
  }, [phase.name]);

  // Closing or reloading the page while a recording is held asks first.
  const holding = phase.name !== 'idle' && phase.name !== 'requesting';
  useEffect(() => {
    if (!holding) return;
    const warn = (event: BeforeUnloadEvent) => event.preventDefault();
    window.addEventListener('beforeunload', warn);
    return () => window.removeEventListener('beforeunload', warn);
  }, [holding]);

  const start = async () => {
    if (disabled || gate.current) return;
    const unavailable = recordingUnavailable();
    if (unavailable) { setProblem(unavailable); return; }
    gate.current = true;
    const version = ++generation.current;
    const current = () => alive.current && version === generation.current;
    setProblem(''); setPhase({ name: 'requesting' });
    let media: MediaStream;
    try { media = await navigator.mediaDevices.getUserMedia({ audio: true }); }
    catch (error) { if (current()) { reset(); setProblem(microphoneProblem(error)); } return; }
    if (!current()) { stopTracks(media); return; }
    const type = recordingTypes.find(candidate => MediaRecorder.isTypeSupported?.(candidate));
    const chunks: Blob[] = [];
    let finish: (file: File) => void = () => {};
    let recording: Recording;
    try {
      const recorder = new MediaRecorder(media, type ? { mimeType: type } : undefined);
      recording = { version, media, recorder, started: Date.now(), seconds: 0, size: 0, stopping: false, stopped: new Promise<File>(resolve => { finish = resolve; }) };
      const live = () => current() && session.current === recording;
      recorder.ondataavailable = event => {
        if (event.data?.size) { chunks.push(event.data); recording.size += event.data.size; }
        if (recording.size > SIZE_LIMIT && !recording.stopping) {
          stop(recording);
          if (live()) { setStream(undefined); setPhase({ name: 'ready', seconds: recording.seconds, reason: 'size' }); }
        }
      };
      recorder.onstop = () => {
        stopTracks(media);
        const mime = recorder.mimeType || type || 'audio/webm';
        finish(new File(chunks, `voice-message.${recordingExtension(mime)}`, { type: mime }));
        // The browser ended it (microphone unplugged or permission revoked): keep what was recorded.
        if (!recording.stopping && live()) { stop(recording); setStream(undefined); setPhase({ name: 'ready', seconds: recording.seconds, reason: 'stopped' }); }
      };
      recorder.onerror = () => { if (live()) setProblem('The microphone stopped unexpectedly. What was recorded is kept.'); };
      recorder.start(1000);
    } catch {
      stopTracks(media); reset(); setProblem('This browser couldn’t record from the microphone. Try again.');
      return;
    }
    session.current = recording; setStream(media); setElapsed(0); setPhase({ name: 'recording' });
  };

  /** The mic again, Escape or Delete: nothing is uploaded and nothing is kept. */
  const cancel = () => {
    if (sending.current) return;
    generation.current++;
    if (session.current) discard(session.current);
    reset();
  };

  /** Send (or Retry): stop, upload once, then send the message with the recording. */
  const submit = async () => {
    const recording = session.current;
    if (!recording || sending.current || disabled) return;
    sending.current = true;
    if (!recording.stopping) stop(recording);
    setStream(undefined); setProblem('');
    setPhase({ name: 'sending', seconds: recording.seconds });
    const live = () => alive.current && session.current === recording;
    try {
      const file = await recording.stopped;
      if (!file.size) { if (live()) { reset(); setProblem('Nothing was recorded. Try again.'); } return; }
      if (!recording.uploaded) {
        abort.current = new AbortController();
        recording.uploaded = await latest.current.upload(file, abort.current.signal);
      }
      if (!live()) return;
      const sent = await latest.current.send(recording.uploaded);
      // The conversation shows why a send was refused; the recording stays for Retry.
      if (live()) { if (sent) reset(); else setPhase({ name: 'failed', seconds: recording.seconds, error: '' }); }
    } catch (error) {
      if (live()) setPhase({ name: 'failed', seconds: recording.seconds, error: errorMessage(error) });
    } finally { sending.current = false; }
  };

  return { phase, elapsed, problem, stream, start, cancel, submit };
}
