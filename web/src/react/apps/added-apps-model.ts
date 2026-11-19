import { identityRequest, workspaceId } from '../../workspace-context';

/** An app in the provider's library, as an admin searching for one sees it
 * (schema 135). Only `addable` apps can be chosen; `reason` says why not. */
export type LibraryApp = { app: string; id: string; name: string; description: string; actions: number;
  state: 'addable' | 'builtin' | 'added' | 'unsupported'; reason: string | null };
export type Effect = 'read' | 'write';
/** One action of a library app to review. `may_read` is false when the library
 * says the action changes something, so it can't be marked as reading. */
export type LibraryAction = { tool: string; title: string; description: string; effect: Effect | null; may_read: boolean;
  available: boolean; reason: string | null };
/** What the admins saved for an app they added. */
export type AddedFact = { revision: number; toolkit: string; actions: { tool: string; title: string; effect: Effect }[];
  added: { at: number; by_name: string | null } };
export type LibraryActions = { app: string; id: string; name: string; description: string; total: number;
  actions: LibraryAction[]; next: string | null; added: AddedFact | null };
/** The most actions one added app has. */
export const MAX_ACTIONS = 20;

const base = () => `/workspaces/${workspaceId}/apps`;
export const searchLibrary = (query: string) =>
  identityRequest<{ apps: LibraryApp[] }>(`${base()}/library?q=${encodeURIComponent(query)}`);
export const loadLibraryActions = (app: string, query: string, page?: string) =>
  identityRequest<LibraryActions>(`${base()}/library/${encodeURIComponent(app)}/actions?q=${encodeURIComponent(query)}${page ? `&page=${encodeURIComponent(page)}` : ''}`);
/** Adds a library app with the chosen actions (`revision` 0), or changes the
 * actions of an added app, which ends every connection of it. */
export const saveAddedApp = (input: { app: string; actions: { tool: string; effect: Effect }[]; revision: number }) =>
  identityRequest<{ saved: true; app: string; name: string; ended: number; provider_revoked: boolean }>(`${base()}/added`, input);
/** Removes an added app; every connection of it ends. */
export const removeAddedApp = (id: string, revision: number) =>
  identityRequest<{ removed: true; ended: number }>(`${base()}/added/${encodeURIComponent(id)}/remove`, { revision });
