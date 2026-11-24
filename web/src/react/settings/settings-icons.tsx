import type { Section } from './settings-sections';

// One stroke icon per Settings page, drawn like the rail's icons.
const paths: Record<Section, string> = {
  profile: 'M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8ZM4 21a8 8 0 0 1 16 0',
  preferences: 'M6 16v-5a6 6 0 0 1 12 0v5l2 2H4l2-2ZM10 21h4',
  'this-workspace': 'M4 21V4h10v17M14 9h6v12M2 21h20M7.5 8h3M7.5 12h3M7.5 16h3',
  workspace: 'M4 21V4h10v17M14 9h6v12M2 21h20M7.5 8h3M7.5 12h3M7.5 16h3',
  people: 'M9 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7ZM2 20a7 7 0 0 1 14 0M16 4.5a3.5 3.5 0 0 1 0 6.5M18 14a6 6 0 0 1 4 6',
  connections: 'M9 3v5M15 3v5M6 8h12v3a6 6 0 0 1-12 0V8ZM12 17v4',
  'company-rules': 'M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6l8-3ZM9 12l2 2 4-4',
  memory: 'M9 4a3 3 0 0 0-3 3 3 3 0 0 0-2 5 3 3 0 0 0 2 5 3 3 0 0 0 6 1V4.5A2.5 2.5 0 0 0 9 4ZM15 4a3 3 0 0 1 3 3 3 3 0 0 1 2 5 3 3 0 0 1-2 5 3 3 0 0 1-6 1',
  usage: 'M4 4v16h16M8 16v-4M12 16V8M16 16v-6',
  'audit-log': 'M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18ZM12 7v5l3 2',
};

export function SettingsIcon({ section }: { section: Section }) {
  return <svg className="settings-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d={paths[section]} /></svg>;
}
