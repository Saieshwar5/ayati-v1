import type { ChangeBatch } from '@cuckoo/shared';
import { startLive, type LiveStatus } from '../src/live/stream';

function streamOf(chunks: string[], hold = false): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      if (!hold) controller.close();
    },
  });
}

async function until(condition: () => boolean) {
  for (let i = 0; i < 1000 && !condition(); i++) await new Promise(resolve => setTimeout(resolve, 5));
  expect(condition()).toBe(true);
}

jest.setTimeout(15_000);

describe('the live message stream', () => {
  it('publishes batches, resumes after the last cursor and sends the bearer token', async () => {
    const urls: string[] = [];
    const inits: RequestInit[] = [];
    const batches: ChangeBatch[] = [];
    let calls = 0;
    const live = startLive({
      url: cursor => `https://cuckoo.example/events${cursor ? `?after=${cursor}` : ''}`,
      token: () => 'device-token',
      onBatch: batch => batches.push(batch),
      onStatus: () => {},
      onUnauthorized: () => {},
      fetch: async (url, init) => {
        urls.push(url);
        inits.push(init!);
        calls++;
        const body =
          calls === 1
            ? streamOf(['event: changes\nid: 4\ndata: {"cursor":4,"events":[{"kind":"message","chat":"c"}]}\n', '\n'])
            : streamOf([], true);
        return new Response(body, { status: 200 });
      },
    });
    await until(() => batches.length === 1);
    expect(batches[0].events[0]).toEqual({ kind: 'message', chat: 'c' });
    expect((inits[0].headers as Record<string, string>).Authorization).toBe('Bearer device-token');
    expect(inits[0].credentials).toBe('omit');
    // The server closed the stream: the next connection resumes after cursor 4.
    await until(() => urls.length === 2);
    expect(urls[1]).toBe('https://cuckoo.example/events?after=4');
    live.stop();
  });

  it('stops on 401 and pauses without reconnecting', async () => {
    const statuses: LiveStatus[] = [];
    let unauthorized = 0;
    let calls = 0;
    const live = startLive({
      url: () => 'https://cuckoo.example/events',
      token: () => undefined,
      onBatch: () => {},
      onStatus: status => statuses.push(status),
      onUnauthorized: () => unauthorized++,
      fetch: async () => {
        calls++;
        return new Response('{}', { status: 401 });
      },
    });
    await until(() => unauthorized === 1);
    live.pause();
    live.resume();
    await until(() => calls === 2);
    live.stop();
    expect(statuses).toContain('paused');
  });
});
