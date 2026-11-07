import { useEffect, type RefObject } from 'react';

type AudioContextClass = typeof AudioContext;
/** One bar per 100 ms; bars scroll left once the canvas is full. */
const BAR_INTERVAL_MS = 100;
const MAX_BARS = 160;

/** Draws the microphone's live level on a canvas while `stream` records.
 * Web Audio and the animation frame are released when the stream ends or the row unmounts. */
export function useLevelMeter(stream: MediaStream | undefined, canvas: RefObject<HTMLCanvasElement | null>) {
  useEffect(() => {
    const node = canvas.current;
    const Context = window.AudioContext ?? (window as unknown as { webkitAudioContext?: AudioContextClass }).webkitAudioContext;
    if (!stream || !node || !Context || typeof window.requestAnimationFrame !== 'function') return;
    let audio: AudioContext, source: MediaStreamAudioSourceNode, analyser: AnalyserNode, silent: GainNode;
    try {
      audio = new Context();
      source = audio.createMediaStreamSource(stream);
      analyser = audio.createAnalyser(); analyser.fftSize = 1024;
      // Some browsers only process nodes that reach the output; a muted gain keeps it silent.
      silent = audio.createGain(); silent.gain.value = 0;
      source.connect(analyser); analyser.connect(silent); silent.connect(audio.destination);
      void audio.resume?.().catch(() => {});
    } catch { return; }
    const samples = new Uint8Array(analyser.fftSize), levels: number[] = [];
    let peak = 0, last = 0, frame = 0;
    const draw = (now: number) => {
      analyser.getByteTimeDomainData(samples);
      for (const sample of samples) peak = Math.max(peak, Math.abs(sample - 128) / 128);
      if (now - last >= BAR_INTERVAL_MS) {
        // A square root lifts quiet speech so the bars stay readable.
        levels.push(Math.min(1, Math.sqrt(peak))); if (levels.length > MAX_BARS) levels.shift();
        peak = 0; last = now; paint(node, levels);
      }
      frame = window.requestAnimationFrame(draw);
    };
    frame = window.requestAnimationFrame(draw);
    return () => {
      window.cancelAnimationFrame(frame);
      source.disconnect(); analyser.disconnect(); silent.disconnect();
      void audio.close().catch(() => {});
    };
  }, [stream, canvas]);
}

/** Bars in the canvas's text colour, so both themes follow their tokens. */
function paint(canvas: HTMLCanvasElement, levels: number[]) {
  const ratio = window.devicePixelRatio || 1;
  const width = Math.round(canvas.clientWidth * ratio), height = Math.round(canvas.clientHeight * ratio);
  const context = width && height ? canvas.getContext('2d') : null;
  if (!context) return;
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
  context.clearRect(0, 0, width, height);
  context.fillStyle = getComputedStyle(canvas).color;
  const bar = 2 * ratio, step = 4 * ratio;
  levels.slice(-Math.floor(width / step)).forEach((level, index) => {
    const size = Math.max(2 * ratio, level * height);
    context.fillRect(index * step, (height - size) / 2, bar, size);
  });
}
