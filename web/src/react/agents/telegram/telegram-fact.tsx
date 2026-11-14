import { useCallback } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import type { Agent } from '../model';
import { activateTelegram, deactivateTelegram, linkTelegram, loadTelegram, loadTelegramAccount, openTelegram, type TelegramStatus } from './model';
import './telegram.css';

function explain(data: TelegramStatus, linked: boolean | undefined): string {
  if (data.state === 'requested') return data.is_owner
    ? 'Tap Create bot in Telegram to finish. It takes one tap.'
    : data.owner_asked === false
      ? 'Waiting for the workspace’s Telegram owner to create the bot. Ask them to open this agent in Cuckoo.'
      : 'Waiting for the workspace’s Telegram owner to create the bot. They got a button in Telegram.';
  if (data.state === 'active' && linked === false) return 'Connect your Telegram once, then open the agent there. Only members who connected Telegram can talk to it.';
  if (data.state === 'active') return 'Members who connected their Telegram can talk to this agent there.';
  if (data.can_manage && !data.connected) return 'A workspace admin needs to connect Telegram in Settings › Workspace first.';
  return '';
}

/** The agent's Telegram on its Overview: its bot for everyone who can talk to
 * it, with Activate / Switch off for its owner and admins. */
export function TelegramFact({ agent }: { agent: Agent }) {
  // A stable loader per agent, so the status is read once for each agent shown.
  const load = useCallback(() => loadTelegram(agent.id), [agent.id]);
  const status = useResource(load);
  const account = useResource(loadTelegramAccount);
  const mutation = useMutation();
  const data = status.data;
  if (!data || (!data.offered && data.state === 'off')) return null;
  const save = (action: (agent: string) => Promise<TelegramStatus>) => mutation.run(async () => {
    status.setData(await action(agent.id));
  });
  const connect = () => mutation.run(async () => {
    await openTelegram(async () => (await linkTelegram()).link);
    mutation.setStatus('Tap Start in Telegram, then come back here.');
  });
  const linked = account.data?.linked;
  const value = data.state === 'off' ? 'Off'
    : data.state === 'requested' ? 'Waiting for its bot'
      : <><span className="agent-telegram-name">@{data.username}</span>{data.state === 'paused' && <small> · Paused by an admin</small>}</>;
  return <div className="agent-fact agent-telegram-fact">
    <dt>Telegram</dt>
    <dd>{value}</dd>
    <span className="agent-telegram-actions">
      {data.create_link && <a className="ui-button primary" href={data.create_link} target="_blank" rel="noreferrer">Create bot in Telegram</a>}
      {data.state === 'active' && linked === false && <Button primary disabled={mutation.busy} onClick={() => void connect()}>Connect your Telegram</Button>}
      {data.link && data.state === 'active' && <a className="ui-button" href={data.link} target="_blank" rel="noreferrer">Open in Telegram</a>}
      {data.state === 'requested' && <Button disabled={mutation.busy} onClick={() => void status.refresh().catch(() => {})}>Check again</Button>}
      {data.can_manage && (data.state === 'off'
        ? data.connected && <Button primary disabled={mutation.busy} onClick={() => void save(activateTelegram)}>Activate Telegram</Button>
        : <Button disabled={mutation.busy} onClick={() => void save(deactivateTelegram)}>Switch off</Button>)}
    </span>
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status || explain(data, linked)}</p>
  </div>;
}
