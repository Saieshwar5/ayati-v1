import { useState } from 'react';
import { Button } from '../ui';
import { AttentionInbox } from './inbox';
import { attentionKey, attentionLabel } from './model';
import { useAttention } from './use-attention';

export function HomeAttention() {
  const [opened, open] = useState(false);
  const { data, error, loading, refresh } = useAttention('needs_you', null, !opened);
  return <section className="home-notifications" aria-label="Notifications">
    <header className="standard-panel-heading"><h2>Notifications</h2>
      <Button onClick={() => open(true)}>View attention{data?.items.length ? ` · ${data.items.length}${data.next_before ? '+' : ''}` : ''}</Button>
    </header>
    <p className="standard-panel-caption">Agent questions, approvals and outcomes</p>
    {error && <><p role="alert">{error}</p><Button disabled={loading} onClick={() => void refresh()}>Retry attention</Button></>}
    {!data && !error && <p className="home-empty">Checking saved work…</p>}
    {data?.available === false && <p className="home-empty">Notifications need the shared workspace database.</p>}
    {data?.available !== false && data && !data.items.length && <div className="notifications-clear"><span aria-hidden="true">✓</span><strong>Nothing needs your attention</strong><p>Finished work is available in Recent work.</p></div>}
    <div className="notification-preview">{data?.items.slice(0, 3).map(item => <button key={attentionKey(item)} className="notification-row" onClick={() => open(true)}>
      <span className={`notification-mark notice-${item.kind === 'outcome' ? 'problems' : 'decisions'}`} aria-hidden="true">{item.kind === 'outcome' ? '!' : '?'}</span>
      <span className="notification-copy"><strong>{item.title.replace(/^\[Ayati request mode:[^\]]*\]\s*/, '')}</strong><small>{item.agent_name} · {attentionLabel(item)}{!item.read ? ' · Unread' : ''}</small></span>
      <span className="notification-action">Review ↗</span>
    </button>)}</div>
    {opened && <AttentionInbox close={() => open(false)} />}
  </section>;
}
