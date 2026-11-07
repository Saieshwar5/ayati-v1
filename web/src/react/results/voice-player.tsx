import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react';
import type { Attachment } from '../chat/model';
import { voiceClock } from '../chat/voice-recorder';
import { PEAK_BARS, useVoicePeaks } from './voice-peaks';
import './voice-player.css';

/** Recorded in the composer, as opposed to an uploaded audio file. */
export const isVoiceNote = (file: Attachment) => /^voice-message\.(webm|ogg|m4a)$/.test(file.name);

// Only one voice message plays at a time: starting one pauses the other.
let playing: HTMLAudioElement | undefined;
const SEEK_STEP = 5;
// Even, quiet bars hold the waveform's place until it is decoded.
const placeholder: number[] = Array(PEAK_BARS).fill(0.2);

/** The same player for sender and receiver: play/pause, a waveform that fills as it plays
 * (a plain bar when decoding fails), the length, and seeking by click or arrow keys. */
export function VoicePlayer({ file, src }: { file: Attachment; src: string }) {
  const audio = useRef<HTMLAudioElement>(null), root = useRef<HTMLDivElement>(null);
  const [paused, setPaused] = useState(true);
  const [position, setPosition] = useState(0);
  const [mediaLength, setMediaLength] = useState(0);
  const [unplayable, setUnplayable] = useState(false);
  const decoded = useVoicePeaks(`${file.id}:${file.checksum}`, src, root);
  const duration = file.duration_ms ? file.duration_ms / 1000 : decoded && 'duration' in decoded ? decoded.duration : mediaLength;
  const name = isVoiceNote(file) ? 'voice message' : file.name;
  const played = duration ? Math.min(1, position / duration) : 0;
  const bars = !decoded ? placeholder : 'peaks' in decoded ? decoded.peaks : undefined;
  useEffect(() => {
    const node = audio.current;
    return () => { if (node && !node.paused) node.pause(); if (playing === node) playing = undefined; };
  }, []);
  const toggle = () => {
    const node = audio.current; if (!node) return;
    if (!node.paused) { node.pause(); return; }
    setUnplayable(false);
    void node.play().catch(error => { if (!(error instanceof Error && error.name === 'AbortError')) setUnplayable(true); });
  };
  const seek = (seconds: number) => {
    const node = audio.current; if (!node || !duration) return;
    const next = Math.max(0, Math.min(duration, seconds));
    node.currentTime = next; setPosition(next);
  };
  const keys = (event: KeyboardEvent) => {
    const step = { ArrowRight: SEEK_STEP, ArrowUp: SEEK_STEP, ArrowLeft: -SEEK_STEP, ArrowDown: -SEEK_STEP, PageUp: 30, PageDown: -30 }[event.key];
    if (step !== undefined) seek(position + step);
    else if (event.key === 'Home') seek(0);
    else if (event.key === 'End') seek(duration);
    else return;
    event.preventDefault();
  };
  const click = (event: MouseEvent<HTMLDivElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    if (box.width) seek(((event.clientX - box.left) / box.width) * duration);
  };
  const length = (seconds: number) => Number.isFinite(seconds) && seconds > 0 ? seconds : 0;
  return <><div ref={root} className={`voice-player${paused ? '' : ' is-playing'}`}>
    <audio ref={audio} preload="none" src={src}
      onPlay={event => { const node = event.currentTarget; if (playing && playing !== node) playing.pause(); playing = node; setPaused(false); }}
      onPause={event => { if (playing === event.currentTarget) playing = undefined; setPaused(true); }}
      onEnded={event => { event.currentTarget.currentTime = 0; setPosition(0); }}
      onTimeUpdate={event => setPosition(event.currentTarget.currentTime)}
      onLoadedMetadata={event => setMediaLength(length(event.currentTarget.duration))}
      onDurationChange={event => setMediaLength(length(event.currentTarget.duration))}
      onError={() => setUnplayable(true)} />
    <button type="button" className="voice-play" aria-label={`${paused ? 'Play' : 'Pause'} ${name}`} onClick={toggle}>
      <svg viewBox="0 0 24 24" aria-hidden="true">{paused ? <path d="M8.5 6.2v11.6a.6.6 0 0 0 .9.5l9.2-5.8a.6.6 0 0 0 0-1L9.4 5.7a.6.6 0 0 0-.9.5Z" /> : <path d="M8 6h2.6v12H8zM13.4 6H16v12h-2.6z" />}</svg>
    </button>
    <div className="voice-track" role="slider" tabIndex={0} aria-label={`${name[0].toUpperCase()}${name.slice(1)} position`}
      aria-valuemin={0} aria-valuemax={Math.round(duration)} aria-valuenow={Math.round(Math.min(position, duration))}
      aria-valuetext={duration ? `${voiceClock(position)} of ${voiceClock(duration)}` : voiceClock(position)}
      aria-disabled={duration ? undefined : true} onClick={click} onKeyDown={keys}>
      {bars
        ? <span className="voice-wave" aria-hidden="true">{bars.map((peak, index) => <i key={index} className={index < played * bars.length ? 'is-played' : undefined} style={{ height: `${Math.max(14, Math.round(peak * 100))}%` }} />)}</span>
        : <span className="voice-progress" aria-hidden="true"><span style={{ width: `${played * 100}%` }} /></span>}
    </div>
    <span className="voice-length">{voiceClock(duration || position)}</span>
  </div>{unplayable && <small className="voice-error" role="alert">This voice message can’t be played here.</small>}</>;
}
