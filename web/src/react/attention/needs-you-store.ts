// The first page of "Needs me", shared by everything that shows how much needs
// the person: Home and Activity publish each read they make, and the rail reads
// on its own only when nothing else has lately (needs-you-count.ts).
import type { AttentionPage } from './model';

let page: AttentionPage | undefined;
let publishedAt = -Infinity;
const listeners = new Set<() => void>();

export const currentNeedsYou = () => page;
/** When a page was last published, by anyone. */
export const needsYouPublishedAt = () => publishedAt;

/** A first page of `needs_you` was read, or `undefined` when access ended. A
 * page of another shape is ignored rather than shown. */
export function publishNeedsYou(next: AttentionPage | undefined) {
  if (next !== undefined && !Array.isArray(next?.items)) return;
  page = next; publishedAt = next ? Date.now() : -Infinity;
  listeners.forEach(listener => listener());
}

export function subscribeNeedsYou(listener: () => void): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}
