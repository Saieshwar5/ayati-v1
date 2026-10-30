// Settings pages and the links that open them. One sidebar replaced the two rows of
// tabs on 2026-10-30 (D232); links from that layout still open the right page.
export type Section = 'profile' | 'preferences' | 'help' | 'this-workspace'
  | 'workspace' | 'people' | 'connections' | 'usage' | 'billing' | 'audit-log';

export const labels: Record<Section, string> = {
  profile: 'Profile', preferences: 'Preferences', help: 'Help', 'this-workspace': 'This workspace',
  workspace: 'Workspace', people: 'People', connections: 'Connections', usage: 'Usage', billing: 'Billing', 'audit-log': 'Audit log',
};
export const personal: Section[] = ['profile', 'preferences', 'help'];
export const adminWorkspace: Section[] = ['workspace', 'people', 'connections', 'usage', 'billing', 'audit-log'];
export const memberWorkspace: Section[] = ['this-workspace'];
export const comingLater: ReadonlySet<Section> = new Set(['usage', 'billing']);

const oldSections: Record<string, Section> = {
  'my-account': 'profile', appearance: 'preferences', 'help-&-feedback': 'help',
  general: 'workspace', access: 'people', activity: 'audit-log', teams: 'workspace',
};
const oldViews: Record<string, Section> = { people: 'people', users: 'people', company: 'workspace', feedback: 'help' };

/** The page a URL asks for, or undefined when it names none (phones then show the list). */
export function requestedSection(url: URL): Section | undefined {
  const view = url.searchParams.get('view') ?? '';
  if (oldViews[view]) return oldViews[view];
  if (url.hash === '#theme') return 'preferences';
  const asked = url.searchParams.get('section');
  if (!asked) return undefined;
  return asked in labels ? asked as Section : oldSections[asked];
}

export const sectionUrl = (section?: Section) => `/?view=settings${section ? `&section=${section}` : ''}`;
