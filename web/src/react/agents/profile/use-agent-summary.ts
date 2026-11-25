import { useCallback, useEffect } from 'react';
import { identityRequest, workspaceId } from '../../../workspace-context';
import { useResource } from '../../requests';
import { subscribeChanges } from '../../chat/live';
import { loadControls, type ToolConnection } from '../control-model';
import { loadAgentSkills, type Skill } from '../skills/model';
import { loadEvents, type EventTrigger } from '../automation/event-model';
import { loadOwnComputers, ownComputerOf, type OwnComputer } from '../computer/own-computers';
import { loadRequests, type AgentRequest } from '../request-model';
import { base, type Agent } from '../model';
import { loadTasks, type TaskItem } from '../../activity/model';

/** What one agent's page shows in its rows. A part the viewer may not read, or
 * that failed, is left out; the rest of the page still shows. */
export type AgentSummary = {
  skills: Skill[]; notes: number; documents: number;
  /** Lessons waiting for a decision, and whether lessons exist here at all. */
  lessons: { waiting: number; available: boolean };
  apps: ToolConnection[]; servers: ToolConnection[]; ownComputer?: OwnComputer; canAddComputer: boolean;
  triggers: EventTrigger[]; requests: AgentRequest[];
  /** Its open tasks that run on their own: repeating, with a run booked, or woken by an
   * event trigger (designs/agent-tasks; triggers got one in schema 148). */
  tasks: TaskItem[];
};

const settled = async <T,>(work: Promise<T>): Promise<T | undefined> => { try { return await work; } catch { return undefined; } };

async function load(agent: Agent): Promise<AgentSummary> {
  const id = encodeURIComponent(agent.id);
  // Independent reads, together: no part waits for another.
  const [skills, notes, always, learning, controls, own, events, requests, tasks] = await Promise.all([
    settled(loadAgentSkills(agent.id)),
    settled(identityRequest<{ notes: { forgotten: boolean }[] }>(`${base}/${id}/knowledge`)),
    settled(identityRequest<{ items: unknown[] }>(`/workspaces/${workspaceId}/library-always/${id}`)),
    agent.editable ? settled(identityRequest<{ available: boolean; proposals?: { state: string }[] }>(`${base}/${id}/learning`)) : undefined,
    settled(loadControls(agent.id)),
    settled(loadOwnComputers()),
    agent.editable ? settled(loadEvents(agent.id)) : undefined,
    settled(loadRequests(agent.id, agent.editable ? 'visible' : 'mine', 'all')),
    settled(loadTasks('all')),
  ]);
  const connections = controls?.connections ?? [];
  return {
    skills: skills?.skills ?? [],
    notes: notes?.notes?.filter(note => !note.forgotten).length ?? 0,
    documents: always?.items?.length ?? 0,
    lessons: { available: Boolean(learning?.available), waiting: learning?.proposals?.filter(lesson => lesson.state === 'proposed').length ?? 0 },
    apps: connections.filter(connection => connection.provider !== 'system'),
    servers: connections.filter(connection => connection.provider === 'system'),
    ownComputer: ownComputerOf(own, agent.id), canAddComputer: Boolean(own?.can_change),
    triggers: events?.triggers ?? [],
    requests: requests?.requests ?? [],
    tasks: (tasks?.items ?? []).filter(task => task.agent === agent.id
      && (task.repeats || task.next_wakeup_ms || (task.kind === 'commitment' && task.origin === 'event'))),
  };
}

/** The rows' data for one agent, read again when its record changes, when access
 * changes, when the window is looked at again and every 30 seconds while visible
 * (work moves on its own). A panel that changes something asks for a refresh. */
export function useAgentSummary(agent: Agent) {
  const { id, revision, editable } = agent;
  // The agent is read through the ids it is keyed on, so a new object each render reloads nothing.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const read = useCallback(() => load(agent), [id, revision, editable]);
  const resource = useResource(read);
  const { refresh, setData } = resource;
  useEffect(() => {
    const again = () => { if (!document.hidden) void refresh().catch(() => {}); };
    const timer = window.setInterval(again, 30000);
    document.addEventListener('visibilitychange', again);
    const off = subscribeChanges(change => {
      if (change.kind === 'access' || change.kind === 'reset') { setData(undefined); void refresh().catch(() => {}); }
    });
    return () => { clearInterval(timer); document.removeEventListener('visibilitychange', again); off(); };
  }, [refresh, setData]);
  return resource;
}
