import { identityRequest, workspaceId, workspaceUrl } from '../../workspace-context';

export type AttentionView = 'needs_you' | 'recent';
export type AttentionItem = {
  kind: 'question' | 'approval' | 'outcome'; id: string; version: string; title: string;
  status: string; at: number; agent: string; agent_name: string; chat: string;
  thread: string | null; reply: string; conversation_name: string; archived: boolean;
  read: boolean; reviewed: boolean;
};
export type AttentionPage = { available: boolean; items: AttentionItem[]; next_before: string | null };
export const attentionKey = (item: AttentionItem) => `${item.kind}/${item.id}/${item.version}`;
export function loadAttention(view: AttentionView, before: string | null, signal?: AbortSignal) {
  const params = new URLSearchParams({ view });
  if (before) params.set('before', before);
  return identityRequest<AttentionPage>(`/workspaces/${workspaceId}/attention?${params}`, undefined, signal);
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
