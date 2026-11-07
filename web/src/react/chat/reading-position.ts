/** Where the reader is in a scrolling message list: the first message that is at
 * least partly visible and its distance from the list's top edge. Restoring it
 * after older messages are added keeps that message in the same place on screen,
 * whether or not the browser anchors scrolling itself. */
export type ReadingAnchor = { id: string; offset: number };

const messageNodes = (list: HTMLElement) => [...list.querySelectorAll<HTMLElement>('[data-message-id]')];

export function readingAnchor(list: HTMLElement | null): ReadingAnchor | undefined {
  if (!list) return undefined;
  const top = list.getBoundingClientRect().top;
  const node = messageNodes(list).find(item => item.getBoundingClientRect().bottom > top);
  const id = node?.dataset.messageId;
  return node && id ? { id, offset: node.getBoundingClientRect().top - top } : undefined;
}

export function restoreReading(list: HTMLElement | null, anchor: ReadingAnchor | undefined): void {
  if (!list || !anchor) return;
  const node = messageNodes(list).find(item => item.dataset.messageId === anchor.id);
  if (node) list.scrollTop += node.getBoundingClientRect().top - list.getBoundingClientRect().top - anchor.offset;
}
