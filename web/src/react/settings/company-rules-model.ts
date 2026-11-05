import { identityRequest, workspaceId } from '../../workspace-context';
import type { Meaning } from './workspace-connections-model';

export type RuleKind = 'field' | 'daily_total' | 'time';
export type Comparator = '>' | '>=' | '<' | '<=' | '=' | '!=';
export type Effect = 'ask' | 'block';
/** A time rule's window, local to the workspace; days 1 (Monday) to 7 (Sunday), none for every day. */
export type TimeWindow = { from: string; to: string; days: number[] };
export type RuleValue = string | number | boolean | TimeWindow;
export type RuleInput = { id: string; operation: string | null; connection: string | null; meaning: Meaning | null; kind: RuleKind; field: string;
  comparator: Comparator; value: RuleValue; effect: Effect; note: string; enabled: boolean; revision: number };
export type CompanyRule = RuleInput & { summary: string; updated_by_name?: string | null; updated_ms?: number };
export type RuleField = { name: string; type: 'string' | 'number' | 'integer' | 'boolean' };
export type RuleAction = { connection: string; account: string; operation: string; title: string; effect: string; meaning: Meaning; fields: RuleField[] };
export type RulesView = { rules: CompanyRule[]; actions: RuleAction[]; can_manage: boolean };

const path = () => `/workspaces/${workspaceId}/guard/rules`;
export const loadRules = () => identityRequest<RulesView>(path());
export const saveRule = (rule: RuleInput) => identityRequest<CompanyRule>(path(), rule);

export const comparisons: Record<Comparator, string> = { '>': 'is over', '>=': 'is at least', '<': 'is under', '<=': 'is at most', '=': 'is', '!=': 'is not' };
export const numeric = (field?: RuleField) => field?.type === 'number' || field?.type === 'integer';
export const kindLabels: Record<Meaning, string> = { read: 'Any reading action', change: 'Any change', money: 'Any money action', sends_outside: 'Any action that sends outside', delete: 'Any delete' };
export const dayNames = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday'];

function shown(value: RuleValue): string {
  return typeof value === 'string' ? `"${value}"` : String(value);
}
const list = (names: string[]) => names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;

/** The same plain words the server saves (`guard::summary`), for the preview while editing. */
export function readBack(rule: Pick<RuleInput, 'kind' | 'field' | 'comparator' | 'value' | 'effect' | 'meaning'>, action: string): string {
  const target = rule.meaning ? kindLabels[rule.meaning] : action;
  const effect = rule.effect === 'block' ? 'block it' : 'ask first';
  if (rule.kind === 'time') {
    const window = rule.value as TimeWindow;
    const when = window.from === window.to ? 'at any time' : `between ${window.from} and ${window.to}`;
    const days = list([...window.days].sort().map(d => dayNames[d - 1]));
    return days ? `${target}: on ${days}, ${when}, ${effect}.` : `${target}: ${when}, ${effect}.`;
  }
  const field = rule.field.replaceAll('_', ' ');
  if (rule.kind === 'daily_total') return `${target}: if today's total ${field} would ${rule.comparator === '>=' ? 'reach' : 'go over'} ${shown(rule.value)}, ${effect}.`;
  return `${target}: if ${field} ${comparisons[rule.comparator]} ${shown(rule.value)}, ${effect}.`;
}
