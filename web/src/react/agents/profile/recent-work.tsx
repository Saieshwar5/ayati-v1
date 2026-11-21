import { workspaceUrl } from '../../../workspace-context';
import { requestLabels, requestUrl, type AgentRequest } from '../request-model';
import { ProfileIcon, type ProfileIconName } from './profile-parts';

const waiting = ['approval', 'input', 'attention'];
const stopped = ['failed', 'cancelled', 'access_changed'];
const mark = (state: string): [string, ProfileIconName] =>
  waiting.includes(state) ? ['ask', 'ask'] : state === 'complete' ? ['done', 'check'] : stopped.includes(state) ? ['stopped', 'x'] : ['busy', 'spin'];
const time = (ms: number) => {
  const date = new Date(ms), today = new Date().toDateString() === date.toDateString();
  return today ? date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : date.toLocaleDateString([], { day: 'numeric', month: 'short' });
};

/**
 * What the agent did lately, beside its page. It only shows and leads away
 * (2026-11-21): a request that waits for a person opens Activity → Needs me,
 * where it is answered; every other one opens its conversation. Nothing is
 * approved, answered or stopped on the agent's page.
 */
export function RecentWork({ requests, all, loaded }: { requests: AgentRequest[]; /** Its owners and admins see everyone's requests. */ all: boolean; loaded: boolean }) {
  return <section className="profile-work" aria-label={all ? 'Recent work' : 'Your requests'}>
    <header><h3>{all ? 'Recent work' : 'Your requests'}</h3>
      {requests.length > 0 && <a href={workspaceUrl('/?view=activity&tab=history')}>See all</a>}</header>
    {!loaded && <p className="profile-quiet" role="status">Loading…</p>}
    {loaded && !requests.length && <p className="profile-quiet">Nothing yet</p>}
    <ul>{requests.slice(0, 8).map(request => {
      const [kind, icon] = mark(request.state), needs = waiting.includes(request.state);
      return <li key={request.id}><a href={needs ? workspaceUrl('/?view=activity&tab=needs') : requestUrl(request)}
        aria-label={`${request.summary}, ${requestLabels[request.state] ?? request.state}, opens in ${needs ? 'Activity' : 'Chat'}`}>
        <span className={`profile-work-mark ${kind}`}><ProfileIcon name={icon} size={15} /></span>
        <span className="profile-work-text"><strong>{request.summary}</strong>
          <small>{needs && <span className="profile-pill">{requestLabels[request.state]}</span>}{[request.requester_name, request.conversation_name].filter(Boolean).join(' · ')}</small></span>
        <time dateTime={new Date(request.created_ms).toISOString()}>{time(request.created_ms)}</time><ProfileIcon name="out" size={14} />
      </a></li>;
    })}</ul>
  </section>;
}
