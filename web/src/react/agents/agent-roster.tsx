import { useState, type ReactNode } from 'react';
import { Button } from '../ui';
import type { Agent } from './model';
import { AgentAvatar } from './avatar';

/** A list this long gets a search box; a shorter one is quicker to read. */
const SEARCH_FROM = 9;

/** An assistant is named by whose it is, not by a job title. */
export const jobOf = (agent: Agent) => agent.assistant
  ? (agent.is_owner ? 'Your assistant' : `${agent.owner_name ?? 'A member'}’s assistant`) : agent.role;
/** What a row says beside a dot that is not green. */
const stateOf = (agent: Agent) => agent.needs_owner ? 'Needs an owner' : agent.settings.paused ? 'Paused' : agent.pending ? `${agent.pending} working` : '';

/**
 * The agent list (designs/agents-profile, 2026-11-21): your own assistant first,
 * then the team, then (for admins) other members' assistants. Each face carries
 * its state dot, as in Chat. `draft` is the unfinished new agent's row.
 */
export function AgentRoster({ agents, loaded, error, retry, current, locked, draft, begin, select }: {
  agents: Agent[]; loaded: boolean; error?: string; retry: () => void;
  /** The open agent's id. */
  current?: string;
  /** While a new agent is being set up, the others cannot be opened. */
  locked: boolean; draft?: ReactNode; begin: () => void; select: (id: string) => void;
}) {
  const [query, setQuery] = useState('');
  const wanted = query.trim().toLowerCase();
  const shown = agents.filter(agent => `${agent.name} ${jobOf(agent)}`.toLowerCase().includes(wanted));
  const yours = shown.filter(agent => agent.assistant && agent.is_owner);
  const team = shown.filter(agent => !agent.assistant);
  const others = shown.filter(agent => agent.assistant && !agent.is_owner);
  const row = (agent: Agent) => {
    const state = stateOf(agent);
    // Two assistants may share a name, so the label says whose each is.
    const label = [`Open agent ${agent.name}`, agent.assistant && !agent.is_owner && jobOf(agent), state].filter(Boolean).join(', ');
    return <Button className="agent-team-row" key={agent.id} aria-label={label} aria-current={!locked && current === agent.id ? 'true' : undefined}
      disabled={locked} onClick={() => select(agent.id)}>
      <AgentAvatar agent={agent} dot /><span><strong>{agent.name}</strong><small>{jobOf(agent)}</small></span>
      {state && <small className={`agent-roster-state${agent.pending && !agent.settings.paused && !agent.needs_owner ? ' working' : ''}`}>{state}</small>}
    </Button>;
  };
  return <aside className="agent-roster" aria-label="Agent team"><header><h1>Agents</h1>
    <Button primary disabled={!loaded || locked} onClick={begin}>＋ New agent</Button>
  </header>
  {agents.length >= SEARCH_FROM && <input className="ui-search" type="search" aria-label="Find an agent" placeholder="Search agents" value={query} onChange={event => setQuery(event.target.value)} />}
  {error && <p role="alert">{error}<Button onClick={retry}>Retry agents</Button></p>}
  {!loaded && !error && <p role="status">Loading agents…</p>}
  <div className="agent-roster-scroll">
    {yours.length > 0 && <><h2>Yours</h2>{yours.map(row)}</>}
    {loaded && <h2>Team <small>{team.length}</small></h2>}
    {draft}
    {team.map(row)}
    {others.length > 0 && <><h2>Members’ assistants <small>{others.length}</small></h2>{others.map(row)}</>}
    {loaded && wanted && !shown.length && <p className="ui-empty">No matching agents.</p>}
    {loaded && !wanted && !team.length && !draft && <p className="ui-empty">Create your first agent.</p>}
  </div></aside>;
}
