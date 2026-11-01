import { Button } from '../ui';
import { workspaceUrl } from '../../workspace-context';
import { attentionKey, attentionLabel } from './model';
import { useAttention } from './use-attention';

const needsMe = () => workspaceUrl('/?view=activity&tab=needs');

/** Home's small Needs me panel; everything opens Activity (D235). */
export function HomeAttention({ shared }: { shared?: ReturnType<typeof useAttention> } = {}) {
  // Home passes its one read; on its own the panel reads for itself.
  const own = useAttention('needs_you', null, !shared);
  const { data, error, loading, refresh } = shared ?? own;
  const waiting = data?.items.filter(item => item.kind !== 'outcome').length ?? 0;
  return <section className="home-notifications" aria-label="Notifications">
    <header className="standard-panel-heading"><h2>Notifications</h2>
      <a className="ui-button" href={needsMe()}>{waiting ? `${waiting}${data?.next_before ? '+' : ''} ${waiting === 1 && !data?.next_before ? 'needs' : 'need'} you →` : `Open Activity${data?.items.length ? ` · ${data.items.length}${data.next_before ? '+' : ''}` : ''}`}</a>
    </header>
    <p className="standard-panel-caption">Agent questions, approvals and outcomes</p>
    {error && <><p role="alert">{error}</p><Button disabled={loading} onClick={() => void refresh()}>Retry attention</Button></>}
    {!data && !error && <p className="home-empty">Checking saved work…</p>}
    {data?.available === false && <p className="home-empty">Notifications need the shared workspace database.</p>}
    {data?.available !== false && data && !data.items.length && <div className="notifications-clear"><span aria-hidden="true">✓</span><strong>Nothing needs your attention</strong><p>Finished work is in Activity → History.</p></div>}
    <div className="notification-preview">{data?.items.slice(0, 3).map(item => <a key={attentionKey(item)} className="notification-row" href={needsMe()}>
      <span className={`notification-mark notice-${item.kind === 'outcome' ? 'problems' : 'decisions'}`} aria-hidden="true">{item.kind === 'outcome' ? '!' : '?'}</span>
      <span className="notification-copy"><strong>{item.title.replace(/^\[Ayati request mode:[^\]]*\]\s*/, '')}</strong><small>{item.agent_name} · {attentionLabel(item)}{!item.read ? ' · Unread' : ''}</small></span>
      <span className="notification-action">Review ↗</span>
    </a>)}</div>
  </section>;
}
