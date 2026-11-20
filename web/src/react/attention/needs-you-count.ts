import { useSyncExternalStore } from 'react';
import { subscribeChanges } from '../chat/live';
import { loadAttention } from './model';
import { currentNeedsYou, needsYouPublishedAt, subscribeNeedsYou } from './needs-you-store';

/** How often the rail reads "Needs me" itself when Home or Activity has not. */
const QUIET_READ_MS = 30_000;
/** Agent messages change often while they work; one read per this long is enough. */
const AGENT_READ_MS = 3_000;

let users = 0;
let stop: (() => void) | undefined;

/** Reads while anything shows the count: at once, when an agent's message
 * changes (a question, an approval, a result), on focus and access changes, and
 * every 30 s while visible if no other read was published meanwhile. */
function start(): () => void {
  const abort = new AbortController();
  let busy = false, again = false, quiet: number | undefined, agent: number | undefined, lastAgentRead = 0;
  const read = async () => {
    if (busy) { again = true; return; }
    busy = true;
    try { await loadAttention('needs_you', null, abort.signal); }
    catch { /* loadAttention publishes the outcome; an outage keeps the last count. */ }
    finally { busy = false; if (again && !abort.signal.aborted) { again = false; void read(); } }
  };
  // A chain of timeouts rather than an interval: each read pushes the next one back.
  const scheduleQuiet = () => {
    window.clearTimeout(quiet);
    quiet = window.setTimeout(() => {
      if (!document.hidden && Date.now() - needsYouPublishedAt() >= QUIET_READ_MS) void read();
      scheduleQuiet();
    }, QUIET_READ_MS);
  };
  const agentChanged = () => {
    if (agent !== undefined) return;
    const wait = Math.max(0, lastAgentRead + AGENT_READ_MS - Date.now());
    agent = window.setTimeout(() => { agent = undefined; lastAgentRead = Date.now(); void read(); }, wait);
  };
  void read(); scheduleQuiet();
  const unsubscribe = subscribeChanges(change => {
    if (change.kind === 'access' || change.kind === 'reset' || change.kind === 'event_access') void read();
    else if (change.kind === 'message' && change.message?.kind === 'agent') agentChanged();
  });
  const focus = () => void read();
  window.addEventListener('focus', focus);
  return () => {
    abort.abort(); unsubscribe(); window.removeEventListener('focus', focus);
    window.clearTimeout(quiet); window.clearTimeout(agent);
  };
}

function subscribe(listener: () => void): () => void {
  const unsubscribe = subscribeNeedsYou(listener);
  if (users++ === 0) stop = start();
  return () => {
    unsubscribe();
    if (--users === 0) { stop?.(); stop = undefined; }
  };
}

/** How many things need the person, as Home and Activity count them: the first
 * page of "Needs me", and whether more pages follow. */
export function useNeedsYouCount(): { count: number; more: boolean } {
  const page = useSyncExternalStore(subscribe, currentNeedsYou);
  return { count: page?.available === false ? 0 : page?.items.length ?? 0, more: Boolean(page?.next_before) };
}
