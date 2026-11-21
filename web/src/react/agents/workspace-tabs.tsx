import { workspaceUrl } from '../../workspace-context';
import type { ProfilePanel } from './profile/profile-rows';

export const workspaceSections = ['Overview', 'Instructions', 'Connections', 'Automations', 'Settings'] as const;
export type WorkspaceSection = typeof workspaceSections[number];
// Older links and saved choices: Tools became Connections, Environment became
// Settings, Activity is part of Overview, and Schedules was Automations' first name.
const formerNames: Record<string, string> = { schedules: 'automations', tools: 'connections', environment: 'settings', activity: 'overview' };
/** A saved or linked section name, under its current or an earlier name. */
export function sectionNamed(value: string | null | undefined): WorkspaceSection | undefined {
  const lower = value?.toLowerCase();
  const name = lower ? formerNames[lower] ?? lower : lower;
  return workspaceSections.find(section => section.toLowerCase() === name);
}
/** A link that opens one agent at one part of its page, e.g. from Activity or Chat; `action`
 * ("connection/operation") picks out one rule on the Connections tab. */
export const agentSectionUrl = (agent: string, section: WorkspaceSection, action?: string) =>
  workspaceUrl(`/?view=agents&selected=${encodeURIComponent(agent)}&section=${section.toLowerCase()}${action ? `&action=${encodeURIComponent(action)}` : ''}`);
/** The agent page has no tabs (2026-11-21): a linked section opens the panel that
 * holds it. Overview and Instructions are the page itself. Connections opens
 * everything it can use, or with `action` ("connection/operation") that one
 * app, with the rule marked. */
export function panelFor(section: WorkspaceSection | undefined, action?: string): ProfilePanel | undefined {
  if (section === 'Connections') return action ? { kind: 'app', id: action.split('/')[0], focus: action } : { kind: 'connections' };
  if (section === 'Automations') return { kind: 'automations' };
  if (section === 'Settings') return { kind: 'model' };
  return undefined;
}
