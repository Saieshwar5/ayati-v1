import { chatRequest } from '../chat/model';

/** A model agents may use; an empty list allows any model. */
export type ModelChoice = { provider: string; model: string };
export type WorkspaceSettings = {
  name: string; timezone: string | null; colour: string | null; approval_backup_hours: number;
  models: ModelChoice[]; default_model: string | null;
  delete_at_ms: number | null; delete_requested_by: string | null;
};
export type SettingsInput = Pick<WorkspaceSettings, 'colour' | 'approval_backup_hours' | 'models' | 'default_model'> & { timezone: string };

export const loadSettings = () => chatRequest<WorkspaceSettings>('/settings');
export const saveSettings = (input: SettingsInput) => chatRequest<WorkspaceSettings>('/settings/workspace', input);
export const scheduleDeletion = (name: string) => chatRequest<{ delete_at_ms: number }>('/settings/deletion', { name });
export const cancelDeletion = () => chatRequest<{ saved: true }>('/settings/deletion/cancel', {});

export const colours: [string, string][] = [['blue', '#2b5cc4'], ['green', '#2f7a55'], ['orange', '#c4562b'], ['purple', '#7a4fb8'], ['brown', '#8a5a2b'], ['teal', '#1f7f8c']];
export const backupChoices = [1, 2, 4, 8, 24, 48, 72, 168];
export const providers: [string, string][] = [['openai', 'OpenAI'], ['fireworks-ai', 'Fireworks']];
export const providerName = (id: string) => providers.find(([value]) => value === id)?.[1] ?? id;
export const hours = (n: number) => n % 24 === 0 && n >= 24 ? `${n / 24} ${n === 24 ? 'day' : 'days'}` : `${n} ${n === 1 ? 'hour' : 'hours'}`;
export const deviceZone = () => Intl.DateTimeFormat().resolvedOptions().timeZone;
/** The whole settings write for a change to some of it; an unset time zone becomes this device's. */
export const settingsInput = (settings: WorkspaceSettings, change: Partial<SettingsInput> = {}): SettingsInput => ({
  timezone: settings.timezone ?? deviceZone(), colour: settings.colour, approval_backup_hours: settings.approval_backup_hours,
  models: settings.models ?? [], default_model: settings.default_model ?? null, ...change,
});

export type ZoneChoice = { id: string; label: string; minutes: number };
const part = (zone: string, style: 'shortOffset' | 'longGeneric', now: Date) =>
  new Intl.DateTimeFormat('en-US', { timeZone: zone, timeZoneName: style }).formatToParts(now).find(item => item.type === 'timeZoneName')?.value ?? '';
/** "(GMT+5:30) Kolkata · India Standard Time", or undefined for a name this browser doesn't know. */
function zoneChoice(id: string, now: Date): ZoneChoice | undefined {
  try {
    const offset = part(id, 'shortOffset', now);
    const [, sign = '+', hours = '0', minutes = '0'] = /^GMT([+-])(\d+)(?::(\d+))?$/.exec(offset) ?? [];
    const name = part(id, 'longGeneric', now);
    const city = (id.split('/').pop() ?? id).replaceAll('_', ' ');
    return { id, minutes: (sign === '-' ? -1 : 1) * (Number(hours) * 60 + Number(minutes)),
      label: `(${offset}) ${city}${name && !name.startsWith('GMT') && name !== city ? ` · ${name}` : ''}` };
  } catch { return undefined; }
}
/** Every time zone the browser knows, west to east, with readable names. `saved` is
 * added when the browser lists the same place under another name (Asia/Kolkata and
 * Asia/Calcutta), so the select always shows what is stored. */
export function timeZones(saved?: string | null, now = new Date()): ZoneChoice[] {
  const device = deviceZone();
  const known = typeof Intl.supportedValuesOf === 'function' ? Intl.supportedValuesOf('timeZone') : [device, 'UTC'];
  const ids = new Set([...known, device, ...(saved ? [saved] : [])]);
  return [...ids].map(id => zoneChoice(id, now)).filter(choice => choice !== undefined)
    .sort((a, b) => a.minutes - b.minutes || a.label.localeCompare(b.label));
}
export const timeIn = (zone: string, now = new Date()) => {
  try { return new Intl.DateTimeFormat(undefined, { timeZone: zone, hour: 'numeric', minute: '2-digit' }).format(now); } catch { return ''; }
};
export const deletionDay = (ms: number) => new Date(ms).toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' });
