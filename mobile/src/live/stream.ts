/**
 * The workspace's live message stream (`…/message-events`), read with a
 * streaming fetch because React Native has no EventSource. It resumes from
 * the last cursor, so a reconnect replays anything missed, and reconnects
 * with backoff. The same rules as the web (web/src/react/chat/live.ts).
 */
import { EventStreamParser, headers, type ChangeBatch, type Fetch } from '@cuckoo/shared';

export type LiveStatus = 'connecting' | 'connected' | 'reconnecting' | 'paused';

export type LiveOptions = {
  url: (cursor?: string) => string;
  token: () => string | undefined;
  fetch: Fetch;
  onBatch: (batch: ChangeBatch) => void;
  onStatus: (status: LiveStatus) => void;
  onUnauthorized: () => void;
};

// The server sends `alive` after 15 s without changes; two missed is a dead stream.
export const SILENT_LIMIT_MS = 45_000;

export function backoff(failures: number): number {
  return Math.min(30_000, 1000 * 2 ** Math.min(failures, 5)) + Math.random() * 500;
}

export function startLive(options: LiveOptions): { stop: () => void; pause: () => void; resume: () => void } {
  let stopped = false;
  let paused = false;
  let cursor: string | undefined;
  let failures = 0;
  let lastEvent = Date.now();
  let abort: AbortController | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;

  const connect = async () => {
    if (stopped || paused) return;
    clearTimeout(retry);
    abort?.abort();
    const current = new AbortController();
    abort = current;
    lastEvent = Date.now();
    options.onStatus(failures ? 'reconnecting' : 'connecting');
    try {
      const response = await options.fetch(options.url(cursor), {
        headers: headers(options.token(), false),
        credentials: 'omit',
        signal: current.signal,
      });
      if (response.status === 401) {
        options.onUnauthorized();
        return;
      }
      if (!response.ok || !response.body) throw new Error(`Live stream answered ${response.status}`);
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      const parser = new EventStreamParser(event => {
        lastEvent = Date.now();
        if (event.event === 'changes') {
          const batch = JSON.parse(event.data) as ChangeBatch;
          // Publish before advancing, so a reconnect replays anything not applied.
          options.onBatch(batch);
          cursor = String(batch.cursor);
          failures = 0;
          options.onStatus('connected');
        } else if (event.event === 'unavailable') {
          options.onBatch({ cursor: Number(cursor ?? 0), events: [{ kind: 'access' }] });
          current.abort();
        }
      });
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        parser.push(decoder.decode(value, { stream: true }));
      }
    } catch {
      // Aborted, offline or the server went away: reconnect below.
    }
    if (stopped || paused || abort !== current) return;
    failures++;
    options.onStatus('reconnecting');
    retry = setTimeout(() => void connect(), backoff(failures));
  };

  const watchdog = setInterval(() => {
    if (!stopped && !paused && Date.now() - lastEvent > SILENT_LIMIT_MS) void connect();
  }, 5_000);
  void connect();

  return {
    stop: () => {
      stopped = true;
      clearTimeout(retry);
      clearInterval(watchdog);
      abort?.abort();
    },
    // The app went to the background: close the stream, keep the cursor.
    pause: () => {
      paused = true;
      clearTimeout(retry);
      abort?.abort();
      options.onStatus('paused');
    },
    resume: () => {
      if (!paused) return;
      paused = false;
      failures = 0;
      void connect();
    },
  };
}
