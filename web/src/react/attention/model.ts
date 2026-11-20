import { identityRequest, workspaceId, workspaceUrl } from '../../workspace-context';
import { publishNeedsYou } from './needs-you-store';

export type AttentionView = 'needs_you' | 'recent';
export type AttentionItem = {
  kind: 'question' | 'approval' | 'outcome'; id: string; version: string; title: string;
  status: string; at: number; agent: string; agent_name: string; chat: string;
  thread: string | null; reply: string; conversation_name: string; archived: boolean;
  read: boolean; reviewed: boolean;
  /** Approvals (Phase 4): the operation, when it was asked, and when an admin may step in. */
  operation?: string; connection?: string; operation_title?: string; backup_at?: number; created_at?: number;
};
export type AttentionPage = { available: boolean; items: AttentionItem[]; next_before: string | null };
export const attentionKey = (item: AttentionItem) => `${item.kind}/${item.id}/${item.version}`;
/** Every read of the first `needs_you` page is shared with the rail and the tab title. */
export async function loadAttention(view: AttentionView, before: string | null, signal?: AbortSignal) {
  const params = new URLSearchParams({ view });
  if (before) params.set('before', before);
  const shared = view === 'needs_you' && !before;
  try {
    const page = await identityRequest<AttentionPage>(`/workspaces/${workspaceId}/attention?${params}`, undefined, signal);
    if (shared && !signal?.aborted) publishNeedsYou(page);
    return page;
  } catch (failure) {
    // Denied or removed access clears the count; an outage keeps the last one.
    if (shared && !signal?.aborted && failure instanceof Error && 'status' in failure && [400, 401, 403, 404].includes(Number(failure.status))) publishNeedsYou(undefined);
    throw failure;
  }
}
export function acknowledgeAttention(item: AttentionItem, reviewed = false) {
  return identityRequest<{ saved: true }>(`/workspaces/${workspaceId}/attention`, {
    kind: item.kind, id: item.id, version: item.version, reviewed,
  });
}
export const attentionSource = (item: AttentionItem) => workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(item.chat)}&message=${encodeURIComponent(item.reply)}${item.thread ? `&thread=${encodeURIComponent(item.thread)}` : ''}`);
export function attentionLabel(item: AttentionItem): string {
  if (item.kind === 'question') return 'Question';
  if (item.kind === 'approval') return item.status === 'uncertain' ? 'Outcome unconfirmed' : 'Exact approval';
  return ({ complete: 'Result ready', failed: 'Failed', interrupted: 'Interrupted' }[item.status] ?? 'Work outcome');
}
