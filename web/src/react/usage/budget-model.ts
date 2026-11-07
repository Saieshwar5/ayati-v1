import { chatRequest } from '../chat/model';

/** A monthly model budget (MG3): the workspace's (`agent` empty) or one agent's, with
 * this calendar month's charges so far. */
export type Budget = {
  agent: string; name: string; monthly_usd: number; used_usd: number; percent: number;
  warn_percent: number; state: 'ok' | 'warning' | 'reached'; month_start_ms: number;
};
export type Budgets = { budgets: Budget[]; can_change: boolean };
export type BudgetInput = { agent: string; monthly_usd: number | null; warn_percent: number };

export const loadBudgets = () => chatRequest<Budgets>('/model-budgets');
export const saveBudget = (input: BudgetInput) => chatRequest<Budgets>('/model-budgets', input);
export const warnChoices = [50, 75, 80, 90, 95];

export function budgetSentence(budget: Budget): string {
  if (budget.state === 'reached') return budget.agent ? 'Used up: this agent takes no new requests until next month or a higher budget.' : 'Used up: agents take no new requests until next month or a higher budget.';
  if (budget.state === 'warning') return `Past ${budget.warn_percent}% of the budget.`;
  return `Warns at ${budget.warn_percent}%.`;
}
