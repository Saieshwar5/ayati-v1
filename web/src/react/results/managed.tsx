import { useMemo, type ReactNode } from 'react';
import { workspaceId } from '../../workspace-context';
import { chatRequest } from '../chat/model';
import { subscribeChanges } from '../chat/live';
import { Results } from './provider';
import type { ResultSource, ViewerImage } from './model';
export function ManagedResults({ chat, changed, images, children }: { chat: string; changed: (open: boolean) => void; images?: () => ViewerImage[]; children: ReactNode }) {
  const source = useMemo<ResultSource>(() => ({
    read: (path, signal) => chatRequest(path, undefined, signal),
    href: path => `/api/identity/workspaces/${workspaceId}${path}`,
    response: id => `/chats/${chat}/messages/${encodeURIComponent(id)}/response`,
    file: id => `/objects/${encodeURIComponent(id)}`,
    subscribe: check => subscribeChanges(change => { if (['access', 'reset'].includes(change.kind)) check(null); else if (change.kind === 'event_access' && change.chat === chat) check(change.source ?? null); }),
  }), [chat]);
  return <Results source={source} changed={changed} images={images}>{children}</Results>;
}
