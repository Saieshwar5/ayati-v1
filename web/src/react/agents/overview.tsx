import { Button } from '../ui';
import { useResource } from '../requests';
import { audienceLabels, type Agent } from './model';
import { useAgentRequests } from './work';
import { RequestRow } from './request-row';
import { loadMembers } from './create/people-step';
import type { WorkspaceSection } from './workspace-tabs';

/** The first readable lines of the instructions, without the section headings. */
export function about(agent: Agent): string {
  return agent.settings.instructions.split('\n').map(line => line.replace(/^[•\-*]\s*/, '').trim())
    .filter(line => line && !line.startsWith('## ') && !line.endsWith(':')).slice(0, 3).join(' ');
}

/** Overview: what it does, your open requests, and key facts with Change links
 * (designs/agents-simplified 5). Approvals and weekly numbers arrive with Activity (M4). */
export function AgentOverview({ agent, open, people }: {
  agent: Agent; open: (section: WorkspaceSection) => void; people: () => void;
}) {
  const requests = useAgentRequests(agent.id, 'mine', 'active');
  const members = useResource(loadMembers);
  const names = (agent.managers ?? []).map(id => members.data?.users.find(member => member.id === id)?.name ?? (members.data ? 'Former member' : '…'));
  const fact = (label: string, value: string, change?: () => void, action = 'Change') => <div className="agent-fact">
    <dt>{label}</dt><dd>{value}</dd>
    {change && <Button className="agent-fact-change" aria-label={`${action} ${label.toLowerCase()}`} onClick={change}>{action}</Button>}
  </div>;
  return <div className="agent-overview">
    <div className="agent-overview-main">
      <section><h3>About</h3><p className="agent-responsibility">{about(agent) || 'No instructions yet.'}</p></section>
      <section>
        <header className="agent-section-heading"><h3>Your open requests</h3><Button onClick={() => open('Activity')}>Open activity</Button></header>
        {requests.error && <p role="alert">{requests.error}</p>}
        {!requests.data && !requests.error && <p role="status">Loading work…</p>}
        {requests.data?.requests.slice(0, 5).map(request => <RequestRow key={request.id} request={request} refresh={requests.refresh} compact />)}
        {requests.data && !requests.data.requests.length && <p className="ui-help">No open requests from you. Open Chat to give {agent.name} work.</p>}
      </section>
    </div>
    <aside className="agent-overview-side">
      <dl className="agent-key-facts">
        {fact('Who can talk to it', audienceLabels[agent.audience], agent.editable ? people : undefined)}
        {fact('Managers', names.join(', ') || 'None', agent.editable ? people : undefined)}
        {fact('Model', agent.model, () => open('Environment'), agent.editable ? 'Change' : 'View')}
        {fact('Tools', 'Rules for each action', () => open('Tools'), 'View')}
        {fact('Schedules', 'Routines and triggers', () => open('Schedules'), 'View')}
        {fact('Knows', agent.settings.memory ? 'Uses approved notes' : 'Approved notes are off', () => open('Instructions'), 'View')}
      </dl>
    </aside>
  </div>;
}
