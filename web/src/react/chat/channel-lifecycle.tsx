import { useCallback, useEffect, useRef } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { chatRequest, type Conversation } from './model';
import type { Lifecycle } from './organization-model';
import { subscribeChanges } from './live';
export function ChannelLifecycle({ chat, close, changed }: { chat: Conversation; close: () => void; changed: () => Promise<void> }) {
  const load = useCallback(() => chatRequest<Lifecycle>(`/chats/${chat.id}/lifecycle`), [chat.id]);
  const resource = useResource(load), mutation = useMutation();
  useEffect(() => subscribeChanges(change => { if (['access', 'reset'].includes(change.kind)) void resource.refresh().catch(() => {}); }), [resource.refresh]);
  const attempt = useRef<{ signature: string; request: string }>(undefined);
  const change = () => {
    if (!resource.data?.manageable) return;
    const payload = { archived: !resource.data.archived, revision: resource.data.revision };
    const signature = JSON.stringify(payload);
    if (attempt.current?.signature !== signature) attempt.current = { signature, request: crypto.randomUUID() };
    const request = attempt.current.request;
    void mutation.run(async () => { await chatRequest(`/chats/${chat.id}/lifecycle`, { ...payload, request }); await changed(); close(); });
  };
  const archived = resource.data?.archived ?? Boolean(chat.archived);
  return <Dialog title={archived ? 'Restore channel' : 'Archive channel'} close={close} busy={mutation.busy} status={mutation.status}
    footer={<><Button disabled={mutation.busy} onClick={() => { mutation.setStatus(''); void resource.refresh().catch(() => {}); }}>Refresh channel state</Button><Button primary disabled={mutation.busy || !resource.data?.manageable} onClick={change}>{archived ? 'Restore channel' : 'Archive channel'}</Button></>}>
    {resource.data && <h3>{chat.name}</h3>}
    {resource.error && <p role="alert">{resource.error}</p>}
    {!resource.data && !resource.error && <p role="status">Loading channel state…</p>}
    {resource.data && <><p>{archived ? 'Restore lets people send new messages and agent requests again.' : 'Archive closes new messages and agent requests for everyone. History and saved files stay available under their existing access settings.'}</p>
      <p>{resource.data.active_requests} visible unfinished {resource.data.active_requests === 1 ? 'request' : 'requests'}. Already accepted work can continue; its questions, approvals and Stop controls stay available.</p>
      <p>{archived ? 'Stopped work and closed approvals stay closed. Restore does not start work or change permissions.' : 'Archive does not stop existing work or approve an action. You can still stop your own requests separately.'}</p>
      {!resource.data.manageable && <p>Only the channel creator or a workspace Owner with channel access can change this state.</p>}
    </>}
  </Dialog>;
}
