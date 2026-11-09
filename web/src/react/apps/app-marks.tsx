import { meanings, type Meaning } from '../settings/workspace-connections-model';
import { appColour, appState, type AppState } from './apps-model';
import './app-marks.css';

const order: Meaning[] = ['read', 'change', 'money', 'sends_outside', 'delete'];

/** A service's tile: its first letter on a stable colour, the same on Apps,
 * in the catalog and wherever agents are set up. */
export function AppLetter({ id, name, size = '' }: { id: string; name: string; size?: '' | 'big' | 'small' }) {
  return <span className={`app-icon app-letter ${size}`} style={{ background: appColour(id) }} aria-hidden="true">{name.trim()[0]?.toUpperCase() ?? '?'}</span>;
}

/** What agents could do with an app, in Controls' words: reads, changes, money… */
export function KindChips({ kinds }: { kinds: Meaning[] }) {
  const present = order.filter(kind => kinds.includes(kind));
  if (!present.length) return null;
  return <span className="kind-chips">{present.map(kind => <span key={kind} className={`kind-chip ${kind}`}>{meanings[kind]}</span>)}</span>;
}

export const stateWords: Record<AppState, string> = { ok: 'Works', reconnect: 'Needs reconnecting', writes_paused: 'Writes paused', paused: 'Paused by Cuckoo' };

/** A service's health where people choose it. Working services show nothing. */
export function HealthTag({ app }: { app: { state?: AppState; status?: string } }) {
  const state = appState(app);
  if (state === 'ok') return null;
  return <span className={`health-tag ${state}`}>{state === 'writes_paused' && <PauseIcon />}{state === 'reconnect' && <RefreshIcon />}{stateWords[state]}</span>;
}

export function PauseIcon() {
  return <svg className="mark-icon" viewBox="0 0 16 16" aria-hidden="true"><path d="M5.5 3.5v9M10.5 3.5v9" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" /></svg>;
}
export function RefreshIcon() {
  return <svg className="mark-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M13 8a5 5 0 1 1-1.5-3.6M13 2.5v3h-3" /></svg>;
}
export function CheckIcon() {
  return <svg className="mark-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="m3.5 8.5 3 3 6-7" /></svg>;
}
export function PlusIcon() {
  return <svg className="mark-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" aria-hidden="true"><path d="M8 3v10M3 8h10" /></svg>;
}
