import { useCallback, useState } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import type { Agent } from '../model';
import { activateEmail, deactivateEmail, loadEmail, type EmailStatus } from './model';
import './email.css';

/** The agent's email address on its Overview: shown to everyone who can talk to
 * it, with Activate / Switch off for its owner and admins. */
export function EmailFact({ agent }: { agent: Agent }) {
  // A stable loader per agent, so the status is read once for each agent shown.
  const load = useCallback(() => loadEmail(agent.id), [agent.id]);
  const status = useResource(load);
  const mutation = useMutation();
  const [copied, setCopied] = useState(false);
  const data = status.data;
  if (!data?.offered && data?.state !== 'active') return null;
  const save = (action: (agent: string) => Promise<EmailStatus>) => mutation.run(async () => {
    status.setData(await action(agent.id));
  });
  const copy = async (address: string) => {
    try { await navigator.clipboard.writeText(address); setCopied(true); }
    catch { mutation.setStatus('Copy didn’t work. Select the address to copy it.'); }
  };
  return <div className="agent-fact agent-email-fact">
    <dt>Email</dt>
    <dd>{data.address
      ? <><span className="agent-email-address">{data.address}</span>
        {data.state === 'paused' && <small> · Paused</small>}</>
      : 'Off'}</dd>
    <span className="agent-email-actions">
      {data.address && <Button onClick={() => void copy(data.address ?? '')}>{copied ? 'Copied' : 'Copy'}</Button>}
      {data.can_manage && (data.address
        ? <Button disabled={mutation.busy} onClick={() => void save(deactivateEmail)}>Switch off</Button>
        : <Button primary disabled={mutation.busy} onClick={() => void save(activateEmail)}>Activate email</Button>)}
    </span>
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status
      || (data.address ? 'People in this workspace can write to this address from their sign-in email.' : '')}</p>
  </div>;
}
