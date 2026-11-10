import { useCallback, useState } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import type { Agent } from '../model';
import { activateWhatsapp, deactivateWhatsapp, displayNumber, loadWhatsapp, type WhatsappStatus } from './model';
import './whatsapp.css';

/** The agent's WhatsApp number on its Overview: shown to everyone who can talk to
 * it, with Activate / Switch off for its owner and admins. */
export function WhatsappFact({ agent }: { agent: Agent }) {
  // A stable loader per agent, so the status is read once for each agent shown.
  const load = useCallback(() => loadWhatsapp(agent.id), [agent.id]);
  const status = useResource(load);
  const mutation = useMutation();
  const [copied, setCopied] = useState(false);
  const data = status.data;
  if (!data?.offered && data?.state !== 'active') return null;
  const save = (action: (agent: string) => Promise<WhatsappStatus>) => mutation.run(async () => {
    status.setData(await action(agent.id));
  });
  const copy = async (number: string) => {
    try { await navigator.clipboard.writeText(number); setCopied(true); }
    catch { mutation.setStatus('Copy didn’t work. Select the number to copy it.'); }
  };
  return <div className="agent-fact agent-whatsapp-fact">
    <dt>WhatsApp</dt>
    <dd>{data.number
      ? <><span className="agent-whatsapp-number">{displayNumber(data.number)}</span>
        {data.state === 'paused' && <small> · Paused by an admin</small>}</>
      : 'Off'}</dd>
    <span className="agent-whatsapp-actions">
      {data.number && <Button onClick={() => void copy(data.number ?? '')}>{copied ? 'Copied' : 'Copy'}</Button>}
      {data.can_manage && (data.number
        ? <Button disabled={mutation.busy} onClick={() => void save(deactivateWhatsapp)}>Switch off</Button>
        : <Button primary disabled={mutation.busy} onClick={() => void save(activateWhatsapp)}>Activate WhatsApp</Button>)}
    </span>
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status
      || (data.number ? 'People who link their WhatsApp number in their profile can message this number.' : '')}</p>
  </div>;
}
