import { AppConnections } from './app-connections';
import type { Agent, Directory } from './model';
import { SystemsSection } from './systems/systems-section';
import { ComputerCard } from './computer/computer-card';
import { ComputerToolsCard } from './computer/computer-controls';
import { OwnComputerCard, loadOwnComputers, ownComputerOf } from './computer/own-computers';
import { useResource } from '../requests';
import { WebSearchCard } from './web-search';

/** Connections: everything this agent can reach, each its own (schema 132).
 * Apps its owner signs in to, the company's MCP servers, its computer and
 * browser, and Cuckoo's built-in tools. Nothing here is shared with another agent.
 * Everything about an agent's computer is here, including the workspace's own
 * machines: there is no workspace-wide computer setting. */
/** `part` shows one group alone in the agent page's panel: one app (by its
 * connection id), the MCP servers, the computers or the built-in tools. */
export type ConnectionPart = { app: string } | 'servers' | 'computers' | 'web';
export function ConnectionsTab({ agent, directory, saved, focus, part, start = false, changed }: {
  agent: Agent; directory: Directory; saved: () => Promise<void>; focus?: string; part?: ConnectionPart; start?: boolean; changed?: () => void;
}) {
  const own = useResource(loadOwnComputers);
  // Admins can always add a machine; members see the card once one exists.
  const showOwn = Boolean(own.data && (own.data.can_change || own.data.computers?.length));
  const refreshOwn = async () => { await own.refresh(); changed?.(); };
  if (typeof part === 'object') return <AppConnections agent={agent} focus={focus} only={part.app} changed={changed} />;
  const servers = <SystemsSection agent={agent} start={start && part === 'servers'} changed={changed} />;
  if (part === 'servers') return servers;
  const computers = (directory.computers || showOwn) && <section className="agent-connection-group" aria-label="Computers">
      <h3 className="agent-group-label">Computers</h3>
      {directory.computers && <>
        <ComputerCard key={agent.id} agent={agent} offer={directory.computers} saved={saved} ownComputer={ownComputerOf(own.data, agent.id)?.name} />
        <ComputerToolsCard agent={agent} saved={saved} />
      </>}
      {showOwn && <OwnComputerCard key={`own-${agent.id}`} agent={agent} view={own.data!} cuckoo={Boolean(directory.computers)} refresh={refreshOwn} />}
    </section>;
  if (part === 'computers') return computers || null;
  const builtIn = directory.web_search && <section className="agent-connection-group" aria-label="Built in">
      <h3 className="agent-group-label">Built in</h3>
      <WebSearchCard agent={agent} saved={saved} />
    </section>;
  if (part === 'web') return builtIn || null;
  return <AppConnections agent={agent} focus={focus} adding={start} changed={changed}>{servers}{computers}{builtIn}</AppConnections>;
}
