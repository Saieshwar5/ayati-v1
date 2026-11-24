import { identityRequest, workspaceId } from '../../../workspace-context';
import type { SkillFile } from '../skills/model';

// Agents that get better on their own (schema 143): an agent's Improving panel,
// its proposed skills, and the workspace's week of learning.
export type Week = {
  answers: number; accepted: number | null; corrected: number | null; followed_up: number | null;
  missed: number; learned: number; held: number; skills_proposed: number; skills_approved: number;
};
export type Review = { id: string; ran_ms: number; requests: number; job_types: number; routine: number; proposed: number; result: string };
export type JobType = { id: string; name: string; runs: number; accepted: number; corrected: number };
export type MadeSkill = {
  id: string; name: string; version: number; created_ms: number; job: string | null;
  before: { runs: number; corrected: number } | null; since: { runs: number; corrected: number } | null;
};
export type Proposal = {
  id: string; kind: 'new' | 'improve' | 'retire'; name: string; description: string; files: SkillFile[]; why: string;
  evidence: { at_ms: number; outcome: string; text: string | null; conversation: string | null }[];
  rehearsals: { job: string; verdict: 'better' | 'same' | 'worse'; note: string | null }[];
  state: 'proposed' | 'approved' | 'rejected'; created_ms: number; decided_ms: number | null; decided_by: string | null;
  job_type: string | null; skill: { id: string; name: string; version: number } | null;
};
export type Configured = { classifier: boolean; reviews: boolean };
export type Improving = {
  available: boolean; this_week?: Week; last_week?: Week; reviews?: Review[]; jobs?: JobType[]; skills?: MadeSkill[];
  proposals?: Proposal[]; configured?: Configured;
};
export type Learning = {
  available: boolean; this_week?: Week; last_week?: Week; configured?: Configured;
  agents?: { id: string; name: string; this_week: Week; last_week: Week }[];
};

const agentPath = (agent: string) => `/workspaces/${workspaceId}/agent-profiles/${encodeURIComponent(agent)}`;
export const loadImproving = (agent: string) => identityRequest<Improving>(`${agentPath(agent)}/improving`);
export const decideProposal = (agent: string, id: string, approve: boolean, files?: SkillFile[]) =>
  identityRequest<{ state: string; skill: string | null }>(`${agentPath(agent)}/skill-proposals/${encodeURIComponent(id)}`, files ? { approve, files } : { approve });
export const loadLearning = () => identityRequest<Learning>(`/workspaces/${workspaceId}/learning`);

/** A measure and the week before it, with whether it got better. */
export function compare(now: number | null, before: number | null, lowerIsBetter: boolean): 'better' | 'worse' | 'same' | 'none' {
  if (now === null || before === null) return 'none';
  if (now === before) return 'same';
  return (now < before) === lowerIsBetter ? 'better' : 'worse';
}
export const when = (ms: number) => new Date(ms).toLocaleDateString([], { day: 'numeric', month: 'short' });
