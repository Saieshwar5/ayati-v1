import { identityRequest, storageScope, workspaceId, workspaceUrl } from '../../workspace-context';
export type Document = Record<string, unknown>;
/** What one agent may do with a folder or item, and whose questions it may answer. */
export type AgentGrant = { agent: string; level: 'read'; use_for: 'viewers' | 'staff' };
export type Item = {
  id: string;
  kind: 'page' | 'file' | 'folder';
  title: string;
  visibility: 'private' | 'selected' | 'workspace';
  revision: number;
  access_revision: number;
  document: Document;
  size: number;
  mime: string;
  readiness: string;
  updated: number;
  manageable: boolean;
  editable: boolean;
  starred: boolean;
  trashed: boolean;
  people: { user: string; access: string; name?: string }[];
  agents: AgentGrant[];
  steward?: string | null;
  agent_count?: number;
  /** The folder this item is filed in; its sharing comes from there. */
  folder?: string | null;
  folder_title?: string | null;
  item_count?: number;
};
export type Listing = { items: Item[]; more: boolean; available: boolean; storage: string };
export const base = `/workspaces/${workspaceId}/library`;
export const request = <T>(path = '', body?: unknown, signal?: AbortSignal) =>
  identityRequest<T>(`${base}${path}`, body, signal);
export const emptyDocument: Document = { type: 'doc', content: [{ type: 'paragraph' }] };
export const itemUrl = (id?: string) =>
  workspaceUrl(`/?view=library${id ? `&item=${encodeURIComponent(id)}` : ''}`);
export const folderUrl = (id: string) =>
  workspaceUrl(`/?view=library&folder=${encodeURIComponent(id)}`);
export const useForName = (value: string) =>
  ({ viewers: 'People who can see it', staff: 'All staff' })[value] ?? value;
export const downloadUrl = (id: string, inline = false) =>
  `/api/identity${base}/${id}/download${inline ? '?inline=true' : ''}`;
export const draftKey = (id: string) => storageScope(`cuckoo.library.draft.${id}`);
export const visibilityName = (value: string) =>
  ({ private: 'Only you', selected: 'Selected people', workspace: 'Workspace' })[value] ?? value;
export const bytesLabel = (size: number) =>
  size >= 1024 * 1024
    ? `${(size / 1024 / 1024).toFixed(1)} MB`
    : `${Math.max(1, Math.round(size / 1024))} KB`;
export function dateLabel(at: number) {
  return new Date(at * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}
export function exportPage(item: Item) {
  const url = URL.createObjectURL(
    new Blob(
      [
        JSON.stringify(
          { title: item.title, revision: item.revision, document: item.document },
          null,
          2,
        ),
      ],
      { type: 'application/json' },
    ),
  );
  const a = document.createElement('a');
  a.href = url;
  a.download = `${item.title.replace(/[^a-z0-9 -]/gi, '_')}.json`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
