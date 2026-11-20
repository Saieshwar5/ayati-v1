// Whether the person is looking at the page, for marking messages read. A
// focused, visible window is. So is a visible window that is not focused but
// that they are using: Cuckoo on a second screen while they type in another
// app, moving the pointer or scrolling over it. The page's own scrolling (to a
// new message, say) is not the person and never counts.

/** How long after the person last used an unfocused window it still counts. */
export const RECENT_USE_MS = 15_000;
const USE_EVENTS = ['pointermove', 'pointerdown', 'wheel', 'touchstart'] as const;

let lastUse = -Infinity;
const listeners = new Set<() => void>();

const recentlyUsed = (now: number) => now - lastUse < RECENT_USE_MS;

/** Whether reading counts now. */
export function attentive(now = Date.now()): boolean {
  return !document.hidden && (document.hasFocus() || recentlyUsed(now));
}

function used() {
  const now = Date.now(), was = recentlyUsed(now);
  lastUse = now;
  // Only the moment the page starts counting matters to listeners; later use
  // only extends it, and the end is checked when a read would happen.
  if (!was) listeners.forEach(listener => listener());
}

/** Hears when a visible, unfocused window starts being used. Returns its cleanup. */
export function subscribeAttention(listener: () => void): () => void {
  if (listeners.size === 0) USE_EVENTS.forEach(name => document.addEventListener(name, used, { capture: true, passive: true }));
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) USE_EVENTS.forEach(name => document.removeEventListener(name, used, { capture: true }));
  };
}
