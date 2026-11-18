import { useResource } from '../requests';
import { workspaceUrl } from '../../workspace-context';
import { agentSectionUrl } from '../agents/workspace-tabs';
import { sectionUrl } from '../settings/settings-sections';
import { loadAppRequests } from './allowed-apps-model';

/** Home's lines about app requests (schema 134): admins see how many wait for
 * them; a person who asked sees the answer for seven days. Nothing shows when
 * there is nothing to say, and a failed read stays quiet. */
export function AppRequestNotices() {
  const { data } = useResource(loadAppRequests);
  const waiting = data?.can_decide ? data.requests.filter(request => request.connector).length + (data.systems_waiting ?? 0) : 0;
  const answers = data?.mine?.filter(request => request.state !== 'pending') ?? [];
  if (!waiting && !answers.length) return null;
  return <section className="home-panel-suggestions" aria-label="App requests">
    {waiting > 0 && <p><span>{waiting} request{waiting === 1 ? '' : 's'} for an app or MCP server {waiting === 1 ? 'is' : 'are'} waiting for your answer.</span>
      <a className="ui-button" href={workspaceUrl(sectionUrl('connections'))}>Review</a></p>}
    {answers.map(request => <p key={request.id}>
      {request.state === 'approved'
        ? <><span>{request.name} is now allowed in this workspace.{request.note ? ` “${request.note}”` : ''}</span>
          {request.agent && <a className="ui-button" href={agentSectionUrl(request.agent, 'Connections')}>Add it to {request.agent_name ?? 'your agent'}</a>}</>
        : <span>Your request for {request.name} was declined.{request.note ? ` “${request.note}”` : ''}</span>}
    </p>)}
  </section>;
}
