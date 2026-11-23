import { useEffect, useRef, useState } from 'react';
import { errorMessage } from '../requests';
import { denied, query, type ResultSource, type SavedFile } from './model';

type Reading = { id: string; file?: SavedFile; error: string };

/** Reads a saved file's details when a viewer opens it, then keeps checking that the reader
 * may still see it: every five seconds while the page is visible, and when access changes.
 * A denied read drops the details, so the viewer stops showing the file.
 * `opened` reads what else the viewer needs once the details are known; its failures are
 * reported like any other and it runs again on retry. */
export function useSavedFile(id: string, source: ResultSource, opened?: (file: SavedFile, signal: AbortSignal) => Promise<void>) {
  const [reading, setReading] = useState<Reading>({ id, error: '' });
  const [attempt, setAttempt] = useState(0);
  const more = useRef(opened); more.current = opened;
  useEffect(() => {
    const abort = new AbortController();
    let checking = false;
    setReading({ id, error: '' });
    const failed = (failure: unknown) => {
      if (abort.signal.aborted) return;
      const gone = denied(failure);
      setReading(current => ({ id, file: gone ? undefined : current.file, error: errorMessage(failure) }));
      return gone;
    };
    void source.read<SavedFile>(query(source.file(id), { metadata: true }), abort.signal)
      .then(file => {
        if (abort.signal.aborted) return;
        setReading({ id, file, error: '' });
        return more.current?.(file, abort.signal);
      })
      .catch(failed);
    const check = async () => {
      if (checking || document.hidden || abort.signal.aborted) return;
      checking = true;
      try { await source.read(query(source.file(id), { access: true }), abort.signal); }
      catch (failure) { if (failed(failure)) abort.abort(); }
      finally { checking = false; }
    };
    const timer = window.setInterval(() => void check(), 5000);
    const unsubscribe = source.subscribe?.(() => void check());
    return () => { abort.abort(); window.clearInterval(timer); unsubscribe?.(); };
  }, [id, source, attempt]);
  // Details read for another file are never shown for this one, even for a single render.
  const current = reading.id === id ? reading : { id, error: '' };
  return { file: current.file, error: current.error, retry: () => setAttempt(value => value + 1) };
}
