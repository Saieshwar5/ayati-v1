import type { AttentionItem } from '@cuckoo/shared';

/** Where an item that needs this person opens: its sheet, or the conversation for an outcome. */
export function attentionTarget(item: AttentionItem) {
  if (item.kind === 'approval') return { pathname: '/approval/[id]' as const, params: { id: item.id } };
  if (item.kind === 'question') return { pathname: '/question/[id]' as const, params: { id: item.id } };
  return { pathname: '/chat/[id]' as const, params: { id: item.chat } };
}
