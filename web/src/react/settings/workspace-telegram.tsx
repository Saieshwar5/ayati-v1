import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { connectWorkspaceTelegram, loadWorkspaceTelegram, openTelegram } from '../agents/telegram/model';
import '../agents/telegram/telegram.css';

/** Settings › Workspace: an admin connects the workspace's Telegram once and
 * becomes its Telegram owner. When someone switches on Telegram for an agent,
 * the owner creates its bot with one tap in Telegram. */
export function WorkspaceTelegram() {
  const resource = useResource(loadWorkspaceTelegram);
  const mutation = useMutation();
  const data = resource.data;
  if (!data?.offered) return null;
  const connect = () => mutation.run(async () => {
    await openTelegram(async () => (await connectWorkspaceTelegram()).link);
    mutation.setStatus('Tap Start in Telegram, then choose Check again.');
  });
  const check = () => mutation.run(async () => {
    const next = await loadWorkspaceTelegram();
    resource.setData(next);
    if (!next.is_owner) mutation.setStatus('Not connected yet. Tap Start in Telegram, then try again.');
  });
  return <section className="settings-telegram" aria-labelledby="settings-workspace-telegram-title">
    <h3 id="settings-workspace-telegram-title">Telegram</h3>
    <div className="settings-telegram-row">
      <p>{data.connected
        ? <>Connected by <strong>{data.owner}</strong>{data.is_owner ? ' (you)' : ''}. Each agent can have its own Telegram bot; {data.is_owner ? 'you create' : `${data.owner} creates`} each one with one tap in Telegram.</>
        : 'Let people talk to agents on Telegram. Connect your Telegram once: you then create each agent’s bot with one tap. Only members who connect their own Telegram can use it.'}</p>
      {data.can_manage && !data.is_owner && <span>
        <Button primary={!data.connected} disabled={mutation.busy} onClick={() => void connect()}>{data.connected ? 'Take over' : 'Connect Telegram'}</Button>{' '}
        <Button disabled={mutation.busy} onClick={() => void check()}>Check again</Button></span>}
    </div>
    <p className="ui-status" role="status">{mutation.busy ? 'Checking…' : mutation.status}</p>
  </section>;
}
