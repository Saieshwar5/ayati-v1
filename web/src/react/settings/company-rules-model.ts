import { identityRequest, workspaceId } from '../../workspace-context';
import type { Meaning } from './workspace-connections-model';

export type RuleKind = 'field' | 'daily_total' | 'time' | 'read_first';
/** A value from an earlier read in the same task, matched on a shared field. */
export type RuleSource = { operation: string; match: string; path?: string; title?: string };
export type Comparator = '>' | '>=' | '<' | '<=' | '=' | '!=';
export type Effect = 'ask' | 'block';
/** A time rule's window, local to the workspace; days 1 (Monday) to 7 (Sunday), none for every day. */
export type TimeWindow = { from: string; to: string; days: number[] };
/** A limit from a Library table row (LB4): the row whose key is the request's `match`. */
export type TableLimit = { table: string; match: string; column: string; title?: string };
export type TableChoice = { id: string; title: string; columns: string[] };
export type RuleValue = string | number | boolean | TimeWindow | TableLimit;
export type RuleInput = { id: string; operation: string | null; connection: string | null; meaning: Meaning | null; source?: RuleSource | null; kind: RuleKind; field: string;
  comparator: Comparator; value: RuleValue; effect: Effect; note: string; enabled: boolean; revision: number;
  /** Turning on with a trial week: it records what it would do, then enforces. */
  trial?: boolean };
export type CompanyRule = RuleInput & { summary: string; updated_by_name?: string | null; updated_ms?: number;
  /** Agents' proposals stay off until an admin reviews them (core-agent CA2). */
  reviewed: boolean; proposal?: string | null; proposed_by_name?: string | null; trial_until_ms?: number | null; trial_matches?: number };
export type Replay = { days: number; examined: number; would_hold: number; would_block: number; summary: string;
  examples: { id: string; title: string; created: number; state: string; effect: Effect; arguments: Record<string, unknown>; day: string }[] };
export type RuleField = { name: string; type: 'string' | 'number' | 'integer' | 'boolean' };
export type RuleAction = { connection: string; account: string; operation: string; title: string; effect: string; meaning: Meaning; fields: RuleField[] };
export type RulesView = { rules: CompanyRule[]; actions: RuleAction[]; can_manage: boolean; tables?: TableChoice[] };

const path = () => `/workspaces/${workspaceId}/guard/rules`;
export const loadRules = () => identityRequest<RulesView>(path());
export const saveRule = (rule: RuleInput) => identityRequest<CompanyRule>(path(), rule);
export const replayRule = (rule: RuleInput) => identityRequest<Replay>(`${path()}/replay`, rule);
export const dismissRule = (id: string) => identityRequest<{ dismissed: boolean }>(`${path()}/${id}/dismiss`, {});

export const comparisons: Record<Comparator, string> = { '>': 'is over', '>=': 'is at least', '<': 'is under', '<=': 'is at most', '=': 'is', '!=': 'is not' };
export const numeric = (field?: RuleField) => field?.type === 'number' || field?.type === 'integer';
export const kindLabels: Record<Meaning, string> = { read: 'Any reading action', change: 'Any change', money: 'Any money action', sends_outside: 'Any action that sends outside', delete: 'Any delete' };
export const dayNames = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];

export const isTableLimit = (value: RuleValue): value is TableLimit => typeof value === 'object' && 'table' in value;
function shown(value: RuleValue): string {
  if (isTableLimit(value)) return `${value.column} in ${value.title ?? 'a table'} for its ${value.match.replaceAll('_', ' ')}`;
  return typeof value === 'string' ? `"${value}"` : String(value);
}
const list = (names: string[]) => names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;

/** The same plain words the server saves (`guard::summary`), for the preview while editing. */
export function readBack(rule: Pick<RuleInput, 'kind' | 'field' | 'comparator' | 'value' | 'effect' | 'meaning' | 'source'>, action: string, readTitle = ''): string {
  const target = rule.meaning ? kindLabels[rule.meaning] : action;
  const effect = rule.effect === 'block' ? 'block it' : 'ask first';
  if (rule.kind === 'time') {
    const window = rule.value as TimeWindow;
    const when = window.from === window.to ? 'at any time' : `between ${window.from} and ${window.to}`;
    const days = list([...window.days].sort().map(d => dayNames[d - 1]));
    return days ? `${target}: on ${days}, ${when}, ${effect}.` : `${target}: ${when}, ${effect}.`;
  }
  const read = rule.source?.title || readTitle || rule.source?.operation || 'a reading action';
  if (rule.kind === 'read_first') return `${target}: unless ${read} for the same ${rule.source?.match.replaceAll('_', ' ')} ran first in this task, ${effect}.`;
  const field = rule.field.replaceAll('_', ' ') + (rule.source ? ` from ${read}` : '');
  if (rule.kind === 'daily_total') return `${target}: if today's total ${field} would ${rule.comparator === '>=' ? 'reach' : 'go over'} ${shown(rule.value)}, ${effect}.`;
  return `${target}: if ${field} ${comparisons[rule.comparator]} ${shown(rule.value)}, ${effect}.`;
}
