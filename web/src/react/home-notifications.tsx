import { Button } from './ui';

export type HomeNotice = { id: string; agent: string; title: string; category: 'Decisions' | 'Problems' | 'Updates' };
export type HomeNotificationsState = { items: HomeNotice[]; total: number };

export function HomeNotifications({ data, open }: {
  data: HomeNotificationsState | null; open: (category: HomeNotice['category']) => void;
}) {
  return <section className="home-notifications" aria-label="Notifications">
    <header className="standard-panel-heading"><h2>Notifications</h2>
      {data && data.total > 0 && <Button onClick={() => open(data.items[0]?.category ?? 'Updates')}>View all · {data.total}</Button>}
    </header>
    <p className="standard-panel-caption">Agent requests and results</p>
    {!data ? <p className="home-empty">Checking notifications…</p>
      : data.total === 0 ? <div className="notifications-clear"><span aria-hidden="true">✓</span><strong>You’re all caught up</strong><p>New agent requests and results will appear here.</p></div>
      : <div className="notification-preview">{data.items.slice(0, 3).map(item => <button key={item.id}
        className="notification-row" onClick={() => open(item.category)}>
        <span className={`notification-mark notice-${item.category.toLowerCase()}`} aria-hidden="true">{item.category === 'Decisions' ? '?' : item.category === 'Problems' ? '!' : '✓'}</span>
        <span className="notification-copy"><strong>{item.title.replace(/^\[Ayati request mode:[^\]]*\]\s*/, '')}</strong><small>{item.agent} · {item.category === 'Decisions' ? 'Approval requested' : item.category === 'Problems' ? 'Needs attention' : 'Result ready'}</small></span>
        <span className="notification-action">{item.category === 'Updates' ? 'View' : 'Review'} ↗</span>
      </button>)}</div>}
  </section>;
}
