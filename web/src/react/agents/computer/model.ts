import { identityRequest } from '../../../workspace-context';
import { base } from '../model';

/** An agent's computer as its owner sets it (runtime/agent-computers.md). */
export type ComputerSettings = { on: boolean; size: string; programs: Programs; browser: Programs; submit: Programs; internet: Internet; websites: string[] };
export type Programs = 'allow' | 'ask' | 'block';
export type Internet = 'none' | 'listed' | 'any';
export const computerDefaults: ComputerSettings = { on: false, size: 'standard', programs: 'allow', browser: 'allow', submit: 'ask', internet: 'none', websites: [] };
/** Agents saved before computers existed have no setting: off. */
export const computerOf = (settings: { computer?: Partial<ComputerSettings> }): ComputerSettings => ({ ...computerDefaults, ...settings.computer });

/** What this Cuckoo offers: sizes with what each costs per hour while working. */
export type ComputerSize = { size: string; cpu: number; memory_gb: number; disk_gb: number; usd_per_hour: number };
export type ComputerOffer = { sleep_after_minutes: number; sizes: ComputerSize[] };

export type ComputerStatus = {
  configured: boolean;
  settings: ComputerSettings;
  /** The workspace's limits (Settings → Computers). */
  policy?: { allowed: boolean; sizes: string[] };
  computer: null | { size: string; state: 'asleep' | 'awake' | 'deleting'; awake_since: number | null; last_used: number; created_at: number; problem: string | null; in_use: boolean };
  month: { seconds: number; charge_usd: number };
  sleep_after_minutes: number | null;
};

export const loadComputer = (agent: string) => identityRequest<ComputerStatus>(`${base}/${agent}/computer`);
export const resetComputer = (agent: string) => identityRequest<ComputerStatus>(`${base}/${agent}/computer/reset`, {});
/** One thing a person does in the agent's browser during a hand-over. */
export type LiveInput = { mouse: { type: 'mousePressed' | 'mouseReleased' | 'mouseMoved'; x: number; y: number } } | { wheel: { x: number; y: number; delta_y: number } } | { text: string } | { key: string };
export const sendLiveInput = (agent: string, input: LiveInput) => identityRequest<{ sent: boolean }>(`${base}/${encodeURIComponent(agent)}/computer/live/input`, input);

export const sizeNames: Record<string, string> = { small: 'Small', standard: 'Standard', large: 'Large' };
export const sizeUses: Record<string, string> = { small: 'Simple files and scripts', standard: 'Most agents', large: 'Big spreadsheets and data work' };
export const internetLabels: Record<Internet, string> = { none: 'No internet', listed: 'Only these websites', any: 'Any website' };

const hourly = new Intl.NumberFormat(undefined, { style: 'currency', currency: 'USD', minimumFractionDigits: 2, maximumFractionDigits: 2 });
export const perHour = (usd: number) => `${hourly.format(usd)} an hour while working`;
export const sizeFacts = (s: ComputerSize) => `${s.cpu} CPU · ${s.memory_gb} GB memory · ${s.disk_gb} GB storage`;

/** "2 h 5 min", "4 min" or "under a minute". */
export function workingTime(seconds: number) {
  const minutes = Math.floor(seconds / 60);
  if (minutes < 1) return seconds > 0 ? 'under a minute' : 'none';
  const hours = Math.floor(minutes / 60);
  return hours ? `${hours} h ${minutes % 60} min` : `${minutes} min`;
}

/** A website as the computer's network rule takes it: a host name, or *.domain. */
export function website(text: string): string | undefined {
  const host = text.trim().toLowerCase().replace(/^https?:\/\//, '').replace(/[/?#].*$/, '');
  const name = host.startsWith('*.') ? host.slice(2) : host;
  const label = /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/;
  return name.includes('.') && name.length <= 253 && name.split('.').every(part => label.test(part)) ? host : undefined;
}
