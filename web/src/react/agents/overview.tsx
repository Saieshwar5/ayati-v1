import { Button } from '../ui';
import { audienceLabels, ownerNames, type Agent } from './model';
import { useAgentRequests } from './work';
import { RequestRow } from './request-row';
import type { WorkspaceSection } from './workspace-tabs';

/** The first readable lines of the instructions, without the section headings. */
export function about(agent: Agent): string {
  return agent.settings.instructions.split('\n').map(line => line.replace(/^[•\-*]\s*/, '').trim())
    .filter(line => line && !line.startsWith('## ') && !line.endsWith(':')).slice(0, 3).join(' ');
}

/** Overview: what it does, your open requests, and key facts with Change links
 * (designs/agents-simplified 5). Approvals and weekly numbers arrive with Activity (M4). */
export function AgentOverview({ agent, open, people, owners }: {
  agent: Agent; open: (section: WorkspaceSection) => void; people: () => void; owners: () => void;
}) {
  const requests = useAgentRequests(agent.id, 'mine', 'active');
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
        {agent.assistant ? fact('Who can talk to it', 'Only its owner') : fact('Who can talk to it', audienceLabels[agent.audience], agent.editable ? people : undefined)}
        {agent.settings.handles && fact('Handles', agent.settings.handles)}
        {fact(agent.owners && agent.owners.length > 1 ? 'Owners' : 'Owner', agent.needs_owner ? 'None: its last owner left'
          : agent.owners ? ownerNames(agent.owners) : agent.owner_name ?? '…', agent.editable && !agent.assistant ? owners : undefined)}
        {fact('Model', agent.model, () => open('Environment'), agent.editable ? 'Change' : 'View')}
        {fact('Tools', 'Rules for each action', () => open('Tools'), 'View')}
        {fact('Automations', 'Schedules and triggers', () => open('Automations'), 'View')}
        {fact('Knows', agent.settings.memory ? 'Uses approved notes' : 'Approved notes are off', () => open('Instructions'), 'View')}
      </dl>
    </aside>
  </div>;
}
