/**
 * What a tapped notification opens (src/identity/push.rs). Its data names only
 * the workspace and the item; the app checks access again when it opens it.
 */
export type NotificationTarget =
  | { workspace: string; kind: 'message'; chat: string }
  | { workspace: string; kind: 'approval'; approval: string }
  | { workspace: string; kind: 'question'; question: string };

const id = (value: unknown): value is string => typeof value === 'string' && /^[A-Za-z0-9_-]{1,100}$/.test(value);

/** Anything unexpected opens nothing. */
export function readTarget(data: unknown): NotificationTarget | undefined {
  if (!data || typeof data !== 'object') return undefined;
  const value = data as Record<string, unknown>;
  if (!id(value.workspace)) return undefined;
  if (value.kind === 'message' && id(value.chat)) return { workspace: value.workspace, kind: 'message', chat: value.chat };
  if (value.kind === 'approval' && id(value.approval)) return { workspace: value.workspace, kind: 'approval', approval: value.approval };
  if (value.kind === 'question' && id(value.question)) return { workspace: value.workspace, kind: 'question', question: value.question };
  return undefined;
}
