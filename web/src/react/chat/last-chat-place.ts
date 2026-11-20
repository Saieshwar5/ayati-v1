import { storageScope } from '../../workspace-context';

/**
 * Where this person last was in Chat (a conversation or For you), so coming
 * back from another page reopens it instead of the first-visit welcome.
 * Per tab and scoped to the user and workspace, like the Agents selection.
 * Only a navigation convenience: it never decides access, the server does.
 */
export type ChatPlace = { kind: 'conversation'; id: string } | { kind: 'for-you' };

const key = () => storageScope('relay.chat.place');

export function rememberChatPlace(place: ChatPlace): void {
  const value = place.kind === 'for-you' ? 'for-you' : `conversation:${place.id}`;
  try { window.sessionStorage.setItem(key(), value); } catch { /* Optional navigation preference. */ }
}

export function rememberedChatPlace(): ChatPlace | undefined {
  let value: string | null = null;
  try { value = window.sessionStorage.getItem(key()); } catch { return undefined; }
  if (value === 'for-you') return { kind: 'for-you' };
  const id = value?.startsWith('conversation:') ? value.slice('conversation:'.length) : '';
  return id ? { kind: 'conversation', id } : undefined;
}

export function forgetChatPlace(): void {
  try { window.sessionStorage.removeItem(key()); } catch { /* Optional navigation preference. */ }
}

/** The Chat URL (relative to the workspace) that opens a remembered place. */
export function chatPlaceUrl(place: ChatPlace): string {
  return place.kind === 'for-you' ? '/?view=chat&section=for-you' : `/?view=chat&conversation=${encodeURIComponent(place.id)}`;
}
