import { useEffect, useState, type RefObject } from 'react';

/** Waveform bars per voice message. */
export const PEAK_BARS = 40;
export type Peaks = { peaks: number[]; duration: number } | { failed: true };
type OfflineContextClass = typeof OfflineAudioContext;

// Decoded once per object and page; a remounted message reuses its waveform.
const cache = new Map<string, Peaks>();
const CACHE_LIMIT = 200;
// Decoding holds a whole recording in memory, so only a couple run at once.
let running = 0;
const waiting: (() => void)[] = [];

async function oneAtATime<T>(task: () => Promise<T>): Promise<T> {
  if (running >= 2) await new Promise<void>(resolve => waiting.push(resolve));
  running++;
  try { return await task(); } finally { running--; waiting.shift()?.(); }
}

/** The loudest sample in each of `count` equal slices, scaled so the loudest bar is 1. */
export function summarizePeaks(samples: Float32Array, count = PEAK_BARS): number[] {
  const size = Math.max(1, Math.floor(samples.length / count)), peaks: number[] = [];
  for (let bar = 0; bar < count; bar++) {
    let peak = 0;
    for (let index = bar * size, end = Math.min(samples.length, index + size); index < end; index++) peak = Math.max(peak, Math.abs(samples[index]));
    peaks.push(peak);
  }
  const loudest = Math.max(0.01, ...peaks);
  return peaks.map(peak => peak / loudest);
}

async function decodePeaks(src: string, signal: AbortSignal): Promise<Peaks> {
  const Offline = window.OfflineAudioContext ?? (window as unknown as { webkitOfflineAudioContext?: OfflineContextClass }).webkitOfflineAudioContext;
  if (!Offline) return { failed: true };
  const response = await fetch(src, { signal });
  if (!response.ok) return { failed: true };
  const bytes = await response.arrayBuffer();
  signal.throwIfAborted();
  // A low rate keeps a five-minute recording small in memory; peaks don't need more.
  const context = new Offline(1, 1, 16000);
  const buffer = await new Promise<AudioBuffer>((resolve, reject) => {
    // Older Safari only reports through the callbacks.
    context.decodeAudioData(bytes, resolve, reject)?.catch?.(reject);
  });
  return { peaks: summarizePeaks(buffer.getChannelData(0)), duration: buffer.duration };
}

/** Decodes the recording's waveform when its player scrolls into view; `failed` falls back to a bar. */
export function useVoicePeaks(key: string, src: string, target: RefObject<Element | null>): Peaks | undefined {
  const [result, setResult] = useState(() => cache.get(key));
  useEffect(() => {
    const node = target.current;
    if (cache.has(key) || !node) return;
    const abort = new AbortController();
    let observer: IntersectionObserver | undefined;
    const load = () => {
      observer?.disconnect();
      void oneAtATime(() => abort.signal.aborted ? Promise.reject(abort.signal.reason) : decodePeaks(src, abort.signal))
        .catch((): Peaks | undefined => abort.signal.aborted ? undefined : { failed: true })
        .then(value => {
          if (!value) return;
          cache.set(key, value); if (cache.size > CACHE_LIMIT) cache.delete(cache.keys().next().value!);
          if (!abort.signal.aborted) setResult(value);
        });
    };
    if (typeof IntersectionObserver === 'undefined') load();
    else { observer = new IntersectionObserver(entries => { if (entries.some(entry => entry.isIntersecting)) load(); }, { rootMargin: '200px' }); observer.observe(node); }
    return () => { abort.abort(); observer?.disconnect(); };
  }, [key, src, target]);
  return result;
}
