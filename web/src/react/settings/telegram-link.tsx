import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { linkTelegram, loadTelegramAccount, openTelegram, unlinkTelegram } from '../agents/telegram/model';
import '../agents/telegram/telegram.css';

/** Profile: your Telegram account. You connect it once by tapping Start on
 * Cuckoo's bot; after that, agents with Telegram know your messages are yours.
 * Without it you can't talk to agents on Telegram. */
export function TelegramLink() {
  const account = useResource(loadTelegramAccount);
  const mutation = useMutation();
  const data = account.data;
  if (!data || (!data.offered && !data.linked)) return null;
  const connect = () => mutation.run(async () => {
    await openTelegram(async () => (await linkTelegram()).link);
    mutation.setStatus('Tap Start in Telegram, then choose I’ve connected.');
  });
  // Linking happens when Start reaches Cuckoo's bot; this reads the result.
  const check = () => mutation.run(async () => {
    const next = await loadTelegramAccount();
    account.setData(next);
    if (!next.linked) mutation.setStatus('Not connected yet. Tap Start in Telegram, then try again.');
  });
  const unlink = () => mutation.run(async () => {
    account.setData({ ...(await unlinkTelegram()), offered: data.offered });
  });
  return <section className="settings-telegram" aria-labelledby="settings-telegram-title">
    <h3 id="settings-telegram-title">Telegram</h3>
    {data.linked ? <div className="settings-telegram-row">
      <p>Connected{data.username ? <> as <strong>@{data.username}</strong></> : ''}. You can talk to agents that have Telegram.</p>
      <Button disabled={mutation.busy} onClick={() => void unlink()}>Disconnect</Button>
    </div> : <div className="settings-telegram-row">
      <p>Connect your Telegram to talk to agents there. Only people who connected can use agents on Telegram. Bot chats are stored by Telegram and are not end-to-end encrypted.</p>
      <span><Button primary disabled={mutation.busy} onClick={() => void connect()}>Connect Telegram</Button>{' '}
        <Button disabled={mutation.busy} onClick={() => void check()}>I’ve connected</Button></span>
    </div>}
    <p className="ui-status" role="status">{mutation.busy ? 'Checking…' : mutation.status}</p>
  </section>;
}

