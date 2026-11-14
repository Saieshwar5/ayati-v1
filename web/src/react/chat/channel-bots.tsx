// Bots in channel Details (schema 119), for the channel's creator and admins:
// anything that can post to a Slack incoming webhook posts here as the bot.
// The address is shown once, when the bot is added.
import { useCallback, useId, useState } from 'react';
import { useMutation, useResource } from '../requests';
import { Button } from '../ui';
import { chatRequest, type Conversation } from './model';

type Bot = { id: string; name: string; created: number; last_post: number | null; posts_30_days: number };

const when = (seconds: number) => new Date(seconds * 1000).toLocaleString([], { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });

export function ChannelBots({ chat }: { chat: Conversation }) {
  const id = useId();
  const load = useCallback(() => chatRequest<{ bots: Bot[] }>(`/chats/${chat.id}/bots`), [chat.id]);
  const bots = useResource(load);
  const [name, setName] = useState('');
  const [added, setAdded] = useState<{ name: string; url: string }>();
  const [copied, setCopied] = useState(false);
  const [removing, setRemoving] = useState<string>();
  const mutation = useMutation();
  const add = () => void mutation.run(async () => {
    if (!name.trim()) return;
    const answer = await chatRequest<{ bot: Bot; url: string }>(`/chats/${chat.id}/bots`, { name: name.trim() });
    setAdded({ name: answer.bot.name, url: answer.url }); setCopied(false); setName('');
    await bots.refresh().catch(() => {});
  });
  // A malformed answer reads as no bots rather than breaking Details.
  const list = Array.isArray(bots.data?.bots) ? bots.data.bots : [];
  const remove = (bot: Bot) => void mutation.run(async () => {
    await chatRequest(`/chats/${chat.id}/bots/${bot.id}/remove`, {});
    setRemoving(undefined);
    await bots.refresh().catch(() => {});
  });
  const copy = async (url: string) => {
    try { await navigator.clipboard.writeText(url); setCopied(true); } catch { setCopied(false); mutation.setStatus('Copy it from the box instead.'); }
  };
  return <section className="chat-details-section chat-bots" aria-labelledby={`${id}-heading`} aria-busy={mutation.busy}>
    <header className="chat-details-section-header"><h3 id={`${id}-heading`}>Bots</h3><small>Creator and admins</small></header>
    <p className="chat-help">Monitoring, CI, your shop or billing system: anything that can post to a Slack incoming webhook can post here. Bots never ask agents or approve anything.</p>
    {bots.error && <p className="chat-error" role="alert">{bots.error}</p>}
    {list.length ? <ul className="chat-bots-list">
      {list.map(bot => <li key={bot.id}>
        <span><b>{bot.name}</b><small>{bot.last_post ? `Last post ${when(bot.last_post)} · ${bot.posts_30_days} in 30 days` : 'No posts yet'}</small></span>
        {removing === bot.id
          ? <span className="chat-bots-confirm"><Button disabled={mutation.busy} onClick={() => remove(bot)}>Remove</Button><Button disabled={mutation.busy} onClick={() => setRemoving(undefined)}>Keep</Button></span>
          : <Button aria-label={`Remove ${bot.name}`} disabled={mutation.busy} onClick={() => setRemoving(bot.id)}>Remove</Button>}
      </li>)}
    </ul> : bots.data && <p className="chat-help">No bots yet.</p>}
    {removing && <p className="chat-help" role="status">Its address stops working at once. Its messages stay.</p>}
    {added && <div className="chat-bots-added" role="status">
      <b>{added.name} can post to #{chat.name}</b>
      <span>Copy its address now: it is shown only once. Anyone with it can post here as the bot.</span>
      <span className="chat-bots-url"><input readOnly aria-label={`Webhook address for ${added.name}`} value={added.url} onFocus={event => event.currentTarget.select()} />
        <Button onClick={() => void copy(added.url)}>{copied ? 'Copied' : 'Copy'}</Button></span>
      <Button onClick={() => setAdded(undefined)}>Done</Button>
    </div>}
    <form className="chat-bots-add" onSubmit={event => { event.preventDefault(); add(); }}>
      <input aria-label="Bot name" placeholder="Name, like Uptime Monitor" maxLength={60} value={name} disabled={mutation.busy} onChange={event => setName(event.target.value)} />
      <Button type="submit" disabled={mutation.busy || !name.trim()}>{mutation.busy ? 'Adding…' : 'Add a bot'}</Button>
    </form>
    {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
  </section>;
}
