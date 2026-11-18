import { useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { requestApp } from './allowed-apps-model';
import type { CatalogApp } from './apps-model';

/** Ask an admin to allow an app nobody decided about yet (schema 134). The
 * request names the agent it is for; a retry reuses its id, so it asks once. */
export function RequestAppDialog({ app, agent, close, sent }: { app: Pick<CatalogApp, 'id' | 'name'>; agent: string; close: () => void; sent: () => void }) {
  const mutation = useMutation();
  const request = useRef<string>(undefined);
  const [reason, setReason] = useState('');
  const send = () => void mutation.run(async () => {
    request.current ??= crypto.randomUUID();
    await requestApp({ id: request.current, connector: app.id, agent, reason: reason.trim() });
    sent();
  });
  return <Dialog title={`Ask for ${app.name}`} close={close} busy={mutation.busy} status={mutation.status}
    footer={<><Button disabled={mutation.busy} onClick={close}>Cancel</Button>
      <Button primary disabled={mutation.busy} onClick={send}>{mutation.busy ? 'Sending…' : 'Send request'}</Button></>}>
    <div className="connect">
      <p>{app.name} isn’t allowed in this workspace yet. An admin decides, and you’ll see the answer on Home.</p>
      <label className="ui-field"><span>What do you need it for? (optional)</span>
        <textarea className="request-reason" maxLength={500} value={reason} onChange={event => setReason(event.target.value)} /></label>
    </div>
  </Dialog>;
}
