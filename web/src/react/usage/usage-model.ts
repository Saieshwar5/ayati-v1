import { chatRequest } from '../chat/model';

/** Calls, tokens and the charge for them (MG2). Charges are at Cuckoo's catalog
 * prices; what providers charge Cuckoo is never sent to the browser. */
export type UsageTotals = { calls: number; input_tokens: number; output_tokens: number; charge_usd: number };
export type UsageRow = UsageTotals & { key: string; name?: string };
/** `workspace` for admins, `agent` for one agent, `mine` for the use made for the viewer. */
export type UsageReport = {
  days: number; since_ms: number; scope: 'workspace' | 'agent' | 'mine'; totals: UsageTotals;
  by_agent: UsageRow[]; by_model: UsageRow[]; by_person: UsageRow[]; by_day: UsageRow[];
  /** Agent computers' awake time and charges over the same period (schema 103). */
  computers?: ComputerUsage; total_charge_usd?: number;
};
export type ComputerUsage = { seconds: number; charge_usd: number; by_agent: { key: string; name?: string; seconds: number; charge_usd: number }[] };
export const periods = [7, 30, 90];

export function loadUsage(days: number, agent?: string, signal?: AbortSignal) {
  const query = new URLSearchParams({ days: String(days) });
  if (agent) query.set('agent', agent);
  return chatRequest<UsageReport>(`/usage?${query}`, undefined, signal);
}

const money = new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', minimumFractionDigits: 2, maximumFractionDigits: 4 });
/** Dollars, with up to four decimals; a charge too small for that never reads as $0.00. */
export const dollars = (n: number) => n > 0 && n < 0.0001 ? 'under $0.0001' : money.format(n);
export const tokens = (n: number) => n.toLocaleString();
