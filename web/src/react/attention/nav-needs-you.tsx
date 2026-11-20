import { useNeedsYouCount } from './needs-you-count';

/** The rail's Activity label with how many things need the person (Needs me):
 * questions, approvals and outcomes to review. The words are the accessible
 * name; the digits are for sight only. Its own component, so a new count
 * redraws only this label. */
export function ActivityNavLabel() {
  const { count, more } = useNeedsYouCount();
  if (count === 0) return <span>Activity</span>;
  const words = `Activity, ${count}${more ? ' or more' : ''} ${count === 1 && !more ? 'needs' : 'need'} you`;
  return <><span>{words}</span>
    <small className="workspace-nav-unread" aria-hidden="true">{count}{more ? '+' : ''}</small></>;
}
