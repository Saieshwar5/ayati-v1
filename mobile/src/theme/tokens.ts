/**
 * The web's semantic tokens (web/src/workspace.css), as listed in
 * designs/mobile-app/README.md. Never a parallel palette: change both together.
 */
export type Tokens = {
  surface: string;
  canvas: string;
  raised: string;
  soft: string;
  selection: string;
  text: string;
  muted: string;
  border: string;
  borderPanel: string;
  borderControl: string;
  working: string;
  online: string;
  success: string;
  danger: string;
  warning: string;
  /** Text on a primary (filled) button. */
  onPrimary: string;
  primary: string;
};

export const light: Tokens = {
  surface: '#FFFFFF',
  canvas: '#F3F4F5',
  raised: '#FFFFFF',
  soft: '#F7F8F9',
  selection: '#EEF0F2',
  text: '#22252A',
  muted: '#5F6670',
  border: '#E5E7EA',
  borderPanel: '#D1D6DD',
  borderControl: '#AEB8C5',
  working: '#6257CF',
  online: '#1F9D55',
  success: '#26745C',
  danger: '#A42F35',
  warning: '#945B10',
  primary: '#22252A',
  onPrimary: '#FFFFFF',
};

export const dark: Tokens = {
  surface: '#1C1F26',
  canvas: '#111318',
  raised: '#252A34',
  soft: '#252A34',
  selection: '#303540',
  text: '#F0F1F5',
  muted: '#ABB1BF',
  border: '#303540',
  borderPanel: '#414956',
  borderControl: '#697487',
  working: '#AFA5FF',
  online: '#4CC38A',
  success: '#85C9A7',
  danger: '#FFADAF',
  warning: '#EFBC77',
  primary: '#F0F1F5',
  onPrimary: '#1C1F26',
};

/** Type scale and corners from the design README. */
export const type = {
  largeTitle: { fontSize: 30, fontWeight: '700' as const },
  header: { fontSize: 17, fontWeight: '600' as const },
  body: { fontSize: 16, lineHeight: 23 },
  meta: { fontSize: 13 },
  small: { fontSize: 12 },
};

export const radius = { button: 12, card: 16, bubble: 18, sheet: 22 };

/** Every target is at least this tall (design principle 2). */
export const touchTarget = 44;
