import { identityRequest, workspaceId } from '../../workspace-context';

export type RuleKind = 'field' | 'daily_total';
export type Comparator = '>' | '>=' | '<' | '<=' | '=' | '!=';
export type Effect = 'ask' | 'block';
export type RuleValue = string | number | boolean;
export type RuleInput = { id: string; operation: string; connection: string | null; kind: RuleKind; field: string; comparator: Comparator;
  value: RuleValue; effect: Effect; note: string; enabled: boolean; revision: number };
export type CompanyRule = RuleInput & { summary: string; updated_by_name?: string | null; updated_ms?: number };
export type RuleField = { name: string; type: 'string' | 'number' | 'integer' | 'boolean' };
export type RuleAction = { connection: string; account: string; operation: string; title: string; effect: string; fields: RuleField[] };
export type RulesView = { rules: CompanyRule[]; actions: RuleAction[]; can_manage: boolean };

const path = () => `/workspaces/${workspaceId}/guard/rules`;
export const loadRules = () => identityRequest<RulesView>(path());
export const saveRule = (rule: RuleInput) => identityRequest<CompanyRule>(path(), rule);

export const comparisons: Record<Comparator, string> = { '>': 'is over', '>=': 'is at least', '<': 'is under', '<=': 'is at most', '=': 'is', '!=': 'is not' };
export const numeric = (field?: RuleField) => field?.type === 'number' || field?.type === 'integer';

function shown(value: RuleValue): string {
  return typeof value === 'string' ? `"${value}"` : String(value);
}

/** The same plain words the server saves (`guard::summary`), for the preview while editing. */
export function readBack(rule: Pick<RuleInput, 'kind' | 'field' | 'comparator' | 'value' | 'effect'>, action: string): string {
  const field = rule.field.replaceAll('_', ' ');
  const effect = rule.effect === 'block' ? 'block it' : 'ask first';
  if (rule.kind === 'daily_total') return `${action}: if today's total ${field} would ${rule.comparator === '>=' ? 'reach' : 'go over'} ${shown(rule.value)}, ${effect}.`;
  return `${action}: if ${field} ${comparisons[rule.comparator]} ${shown(rule.value)}, ${effect}.`;
}
