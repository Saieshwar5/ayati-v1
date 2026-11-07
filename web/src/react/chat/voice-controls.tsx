import { useLayoutEffect, useRef } from 'react';
import { Button } from '../ui';
import { useLevelMeter } from './voice-level';
import { VOICE_LIMIT_SECONDS, voiceClock, type VoicePhase, type VoiceRecorder } from './voice-recorder';
import './voice.css';

/** The mic beside Send; while recording it becomes the mic with × that cancels. */
export function VoiceButton({ voice, disabled }: { voice: VoiceRecorder; disabled: boolean }) {
  const active = voice.phase.name !== 'idle';
  const label = active ? 'Cancel recording' : 'Record voice message';
  return <Button className={`ui-icon composer-mic${active ? ' is-recording' : ''}`} aria-label={label} title={label}
    disabled={active ? voice.phase.name === 'sending' : disabled} onClick={() => { if (active) voice.cancel(); else void voice.start(); }}>
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <rect x="8.5" y="2.5" width="6" height="11" rx="3" /><path d="M5 10v1.5a6.5 6.5 0 0 0 13 0V10M11.5 18v3.5M8 21.5h7" />
      {active && <><circle className="composer-mic-badge" cx="18.5" cy="18.5" r="4.5" /><path className="composer-mic-cross" d="m16.9 16.9 3.2 3.2m0-3.2-3.2 3.2" /></>}
    </svg>
  </Button>;
}

/** Replaces the text field while the microphone is asked for, recording or sending. */
export function VoiceRow({ voice }: { voice: VoiceRecorder }) {
  const row = useRef<HTMLDivElement>(null), canvas = useRef<HTMLCanvasElement>(null);
  useLevelMeter(voice.stream, canvas);
  // Enter and Escape act on the recording, so take focus from the composer (not from elsewhere).
  useLayoutEffect(() => {
    const node = row.current, active = document.activeElement;
    if (node && (!active || active === document.body || node.closest('form')?.contains(active))) node.focus();
  }, []);
  const { phase, elapsed } = voice, left = VOICE_LIMIT_SECONDS - elapsed;
  return <div ref={row} className={`voice-row is-${phase.name}`} role="group" aria-label="Voice message" tabIndex={-1}>
    <span className="voice-dot" aria-hidden="true" />
    {phase.name === 'requesting' && <span className="voice-status" role="status">Waiting for the microphone…</span>}
    {phase.name === 'sending' && <>
      <span className="voice-timer">{voiceClock(phase.seconds)}</span>
      <progress className="voice-sending" aria-label="Sending voice message" />
      <span className="voice-status" role="status">Sending…</span>
    </>}
    {phase.name === 'recording' && <>
      <span className="voice-timer" role="timer">{voiceClock(elapsed)}</span>
      <canvas ref={canvas} className="voice-level" aria-hidden="true" />
      {left <= 60 && <span className="voice-left" aria-hidden="true">{voiceClock(left)} left</span>}
      <span className="voice-hint" aria-hidden="true">Enter sends · Esc cancels</span>
      {/* Announced at start and twice near the limit, not every second. */}
      <span className="visually-hidden" role="status">{left <= 10 ? '10 seconds left' : left <= 60 ? '1 minute left' : 'Recording. Enter sends, Escape cancels.'}</span>
    </>}
  </div>;
}

const keptReason = { time: 'Recording stops at 5 minutes.', size: 'Recording stops at 10 MiB.', stopped: 'The microphone stopped.' };

/** A stopped recording that hasn't been sent: shown above the text field until Send or Delete. */
export function VoiceDraft({ phase, disabled, retry, remove }: { phase: Extract<VoicePhase, { name: 'ready' | 'failed' }>; disabled: boolean; retry: () => void; remove: () => void }) {
  const failed = phase.name === 'failed';
  return <div className={`voice-draft${failed ? ' is-failed' : ''}`}>
    <span role={failed ? 'alert' : 'status'}>
      {`Voice message ${voiceClock(phase.seconds)} ${failed ? 'not sent' : 'ready to send'}`}
      {failed ? phase.error && <small>{phase.error}</small> : <small>{keptReason[phase.reason]}</small>}
    </span>
    {failed && <Button disabled={disabled} onClick={retry}>Retry</Button>}
    <Button onClick={remove}>Delete</Button>
  </div>;
}
