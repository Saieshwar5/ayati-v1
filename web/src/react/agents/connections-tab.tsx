import { AppConnections } from './app-connections';
import type { Agent, Directory } from './model';
import { SystemsSection } from './systems/systems-section';
import { ComputerCard } from './computer/computer-card';
import { ComputerToolsCard } from './computer/computer-controls';
import { WebSearchCard } from './web-search';

/** Connections: everything this agent can reach, each its own (schema 132).
 * Apps its owner signs in to, the company's MCP servers, its computer and
 * browser, and Cuckoo's built-in tools. Nothing here is shared with another agent. */
export function ConnectionsTab({ agent, directory, saved, focus }: { agent: Agent; directory: Directory; saved: () => Promise<void>; focus?: string }) {
  return <AppConnections agent={agent} focus={focus}>
    <SystemsSection agent={agent} />
    {directory.computers && <section className="agent-connection-group" aria-label="Computers">
      <h3 className="agent-group-label">Computers</h3>
      <ComputerCard key={agent.id} agent={agent} offer={directory.computers} saved={saved} />
      <ComputerToolsCard agent={agent} saved={saved} />
    </section>}
    {directory.web_search && <section className="agent-connection-group" aria-label="Built in">
      <h3 className="agent-group-label">Built in</h3>
      <WebSearchCard agent={agent} saved={saved} />
    </section>}
  </AppConnections>;
}
