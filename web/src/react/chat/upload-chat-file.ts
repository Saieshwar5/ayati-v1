import { workspaceId } from '../../workspace-context';
import type { Attachment } from './model';

/** One file's size limit and how many files one message can share. */
export const MAX_BYTES = 10 * 1024 * 1024, MAX_FILES = 8;

/** Stores one file in the conversation; the message that shares it is sent separately. */
export async function uploadChatFile(chat: string, file: File, signal: AbortSignal): Promise<Attachment> {
  const response = await fetch(`/api/identity/workspaces/${workspaceId}/chats/${chat}/objects?name=${encodeURIComponent(file.name)}`, { method: 'POST', headers: { 'X-Ayati-Request': 'identity', 'Content-Type': file.type || 'application/octet-stream' }, body: file, signal });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error ?? `Upload failed (${response.status}).`);
  return data;
}
