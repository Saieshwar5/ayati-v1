// Settings pages and the links that open them. One sidebar replaced the two rows of
// tabs on 2026-10-30 (D232); the 2026-11-21 redesign renamed three pages (Workspace →
// General, Connections → Apps, Company rules → Agent rules) and removed Help (now in
// the account menu) and Billing. The ids in links did not change.
export type Section = 'profile' | 'preferences' | 'this-workspace'
  | 'workspace' | 'people' | 'connections' | 'company-rules' | 'memory' | 'usage' | 'audit-log';

export const labels: Record<Section, string> = {
  profile: 'Profile', preferences: 'Preferences', 'this-workspace': 'This workspace',
  workspace: 'General', people: 'People', connections: 'Apps', 'company-rules': 'Agent rules', memory: 'Memory', usage: 'Usage', 'audit-log': 'Audit log',
};
export const personal: Section[] = ['profile', 'preferences'];
// Memory (schema 142) is the workspace's memory, for admins only.
export const adminWorkspace: Section[] = ['workspace', 'people', 'connections', 'company-rules', 'memory', 'usage', 'audit-log'];
// Members see the model use made for them on Usage (MG2); admins see the workspace's.
export const memberWorkspace: Section[] = ['this-workspace', 'company-rules', 'usage'];

// Links from earlier layouts. Help's and Billing's links open the nearest page that exists.
const oldSections: Record<string, Section> = {
  'my-account': 'profile', appearance: 'preferences', 'help-&-feedback': 'profile', help: 'profile',
  general: 'workspace', access: 'people', activity: 'audit-log', teams: 'workspace', billing: 'usage',
};
const oldViews: Record<string, Section> = { people: 'people', users: 'people', company: 'workspace', feedback: 'profile', account: 'profile' };

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
