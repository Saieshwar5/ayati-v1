import { useEffect, type RefObject } from 'react';
import { errorMessage } from '../requests';
import { appsRequest, builtPath } from './apps-model';

/** Requests an app may have waiting at once; more are refused, not queued. */
const MAX_WAITING = 8;
/** Preview errors sent per load, at most. */
const MAX_REPORTED = 5;
type Message = {
  cuckoo?: unknown;
  id?: unknown;
  op?: unknown;
  collection?: unknown;
  record?: unknown;
  data?: unknown;
  revision?: unknown;
  mine?: unknown;
  message?: unknown;
};

/** Relays a running app's bridge messages (BA2) to Cuckoo's server and answers them.
 * Only messages from this frame's own window are accepted; the server checks every
 * request against the app's manifest and the person's roles. */
export function useAppBridge(
  frame: RefObject<HTMLIFrameElement | null>,
  id: string,
  preview: boolean,
) {
  useEffect(() => {
    let alive = true;
    let waiting = 0;
    let reported = 0;
    const listener = (event: MessageEvent<Message>) => {
      const source = frame.current?.contentWindow;
      const message = event.data;
      if (!source || event.source !== source || !message || message.cuckoo !== 1) return;
      // Errors are only kept from a draft preview, for its builder; at most a few per load.
      if (message.op === 'error') {
        if (preview && reported < MAX_REPORTED && typeof message.message === 'string') {
          reported++;
          void appsRequest(builtPath(id, '/preview-errors'), { message: message.message }).catch(() => {});
        }
        return;
      }
      if (typeof message.id !== 'number' || typeof message.op !== 'string') return;
      const reply = (body: { ok: boolean; result?: unknown; error?: string }) => {
        if (alive) source.postMessage({ cuckoo: 1, id: message.id, ...body }, '*');
      };
      if (waiting >= MAX_WAITING) {
        reply({ ok: false, error: 'Too many requests at once. Try again.' });
        return;
      }
      waiting++;
      const body = {
        preview,
        op: message.op,
        collection: typeof message.collection === 'string' ? message.collection : '',
        record: typeof message.record === 'string' ? message.record : undefined,
        data: message.data,
        revision: typeof message.revision === 'number' ? message.revision : undefined,
        mine: message.mine === true,
      };
      appsRequest(builtPath(id, '/data'), body)
        .then(
          (result) => reply({ ok: true, result }),
          (error: unknown) => reply({ ok: false, error: errorMessage(error) }),
        )
        .finally(() => {
          waiting--;
        });
    };
    window.addEventListener('message', listener);
    return () => {
      alive = false;
      window.removeEventListener('message', listener);
    };
  }, [frame, id, preview]);
}
