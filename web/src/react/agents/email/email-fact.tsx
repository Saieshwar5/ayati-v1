import { useCallback, useState } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { currentWorkspace } from '../../../workspace-context';
import type { Agent } from '../model';
import { activateEmail, deactivateEmail, loadEmail, type EmailStatus } from './model';
import './email.css';

/** The agent's email address on its Overview: shown with Copy to everyone who
 * can talk to it, with Switch on / Switch off for its owner and admins. Hidden
 * when this server offers no email and the agent never had an address. */
export function EmailFact({ agent }: { agent: Agent }) {
  // A stable loader per agent, so the status is read once for each agent shown.
  const load = useCallback(() => loadEmail(agent.id), [agent.id]);
  const status = useResource(load);
  const mutation = useMutation();
  const [copied, setCopied] = useState(false);
  const data = status.data;
  if (!data?.offered && !data?.address) return null;
  const active = data.state === 'active';
  const save = (action: (agent: string) => Promise<EmailStatus>) => mutation.run(async () => {
    status.setData(await action(agent.id));
  });
  const copy = async (address: string) => {
    try { await navigator.clipboard.writeText(address); setCopied(true); }
    catch { mutation.setStatus('Copy didn’t work. Select the address to copy it.'); }
  };
  const workspace = currentWorkspace()?.name ?? 'this workspace';
  const help = active
    ? `People in ${workspace} email this address from the address they sign in with. Anyone else gets no answer.`
    : data.address ? 'Mail to the address gets no answer while email is off. Switching on brings the same address back.'
      : `Gives ${agent.name} its own email address.`;
  return <div className="agent-fact agent-email-fact">
    <dt>Email</dt>
    <dd>{active && data.address ? <span className="agent-email-address"><Address value={data.address} /></span>
      : data.address ? <>Off · <Address value={data.address} /></> : 'Off'}</dd>
    <span className="agent-email-actions">
      {active && data.address && <Button className="agent-fact-change" onClick={() => void copy(data.address ?? '')}>{copied ? 'Copied' : 'Copy'}</Button>}
      {data.can_manage && (active
        ? <Button className="agent-fact-change" disabled={mutation.busy} onClick={() => void save(deactivateEmail)}>Switch off</Button>
        : <Button className="agent-fact-change" disabled={mutation.busy} onClick={() => void save(activateEmail)}>Switch on</Button>)}
    </span>
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status || help}</p>
  </div>;
}

/** An address that breaks before its `@` on narrow screens, never mid-word. */
function Address({ value }: { value: string }) {
  const at = value.indexOf('@');
  return at < 0 ? <>{value}</> : <>{value.slice(0, at)}<wbr />{value.slice(at)}</>;
}
