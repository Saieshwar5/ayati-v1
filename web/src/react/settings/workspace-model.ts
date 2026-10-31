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
/** Every IANA time zone the browser knows, with the device's first. */
export function timeZones(): string[] {
  const device = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const all = typeof Intl.supportedValuesOf === 'function' ? Intl.supportedValuesOf('timeZone') : [device, 'UTC'];
  return [device, ...all.filter(zone => zone !== device)];
}
export const deletionDay = (ms: number) => new Date(ms).toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' });
