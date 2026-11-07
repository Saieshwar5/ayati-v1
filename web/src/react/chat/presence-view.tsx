// How presence and status read on screen (designs/chat-and-account screens 2, 3
// and 10): a green dot on the face, the status emoji after the name, and one
// line in a direct message's header. Status text is the person's own words, so
// it is only ever rendered as text.
import { Fragment } from 'react';
import { activeStatus, shownOnline, usePresence, type Presence, type Status, type StatusKind } from './presence';
import './presence.css';

export type Preset = { kind: Exclude<StatusKind, 'custom'>; emoji: string; label: string; help: string };
export const presets: Preset[] = [
  { kind: 'working', emoji: '💻', label: 'Working', help: 'Shows you’re on it; replies may be slower.' },
  { kind: 'busy', emoji: '⛔', label: 'Busy', help: 'Quieter: only direct messages and @mentions alert you.' },
  { kind: 'meeting', emoji: '🗓️', label: 'In a meeting', help: 'A calendar mark next to your name.' },
  { kind: 'away', emoji: '🌙', label: 'Away', help: 'Shows you as away, even with Cuckoo open.' },
];
const custom = { emoji: '💬', label: 'Status' };

export const statusEmoji = (status: Status) => status.emoji || (presets.find(preset => preset.kind === status.kind) ?? custom).emoji;
export const statusLabel = (status: Status) => status.text || (presets.find(preset => preset.kind === status.kind) ?? custom).label;

const time = (at: number) => new Date(at).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();
const midnightAfter = (day: Date) => new Date(day.getFullYear(), day.getMonth(), day.getDate() + 1).getTime();

/** "until 11:30 AM", "until tonight", "until Sunday", or nothing for a status that stays. */
export function clearsLabel(status: Status, now = Date.now()): string {
  const at = status.clear_at;
  if (at == null) return '';
  const end = new Date(at), today = new Date(now);
  if (at === midnightAfter(today)) return 'until tonight';
  if (sameDay(end, today)) return `until ${time(at)}`;
  if (at - now < 7 * 86_400_000) return `until ${end.toLocaleDateString(undefined, { weekday: 'long' })}`;
  return `until ${end.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}`;
}

/** The account menu's wording (screen 9): "Clears tonight", "Clears at 11:30 AM", "Clears Sunday". */
export function clearsPhrase(status: Status, now = Date.now()): string {
  const when = clearsLabel(status, now).replace(/^until /, '');
  return when ? `Clears ${/^\d/.test(when) ? 'at ' : ''}${when}` : '';
}

/** "Last seen 9:58 AM" today, "Last seen yesterday", otherwise the date. */
export function lastSeenLabel(at: number, now = Date.now()): string {
  const seen = new Date(at), today = new Date(now);
  if (sameDay(seen, today)) return `Last seen ${time(at)}`;
  if (sameDay(seen, new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1))) return 'Last seen yesterday';
  return `Last seen ${seen.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: seen.getFullYear() === today.getFullYear() ? undefined : 'numeric' })}`;
}

/** Words for a screen reader after a name: "online, In a meeting". */
export function presenceWords(presence: Presence | undefined, now = Date.now()): string {
  const status = activeStatus(presence, now);
  return [shownOnline(presence, now) ? 'online' : '', status ? statusLabel(status) : ''].filter(Boolean).join(', ');
}

/** A person's name followed by their presence, e.g. "Meera Iyer, online, In a meeting". */
export function presenceName(name: string, presence: Presence | undefined): string {
  const words = presenceWords(presence);
  return words ? `${name}, ${words}` : name;
}

/** The green dot inside a person's face; nothing when they are away or hidden. */
export function PresenceDot({ id }: { id: string }) {
  return shownOnline(usePresence(id)) ? <span className="presence-dot" aria-hidden="true" /> : null;
}

/** The status emoji after a name, with its text on hover. */
export function StatusEmoji({ id }: { id: string }) {
  const status = activeStatus(usePresence(id));
  if (!status) return null;
  const label = statusLabel(status);
  return <span className="presence-status-emoji" title={label} aria-hidden="true">{statusEmoji(status)}</span>;
}

/** A suggestion's status in the @mention list: "🗓️ In a meeting", and "online" for screen readers. */
export function MentionStatus({ id }: { id: string }) {
  const presence = usePresence(id);
  const status = activeStatus(presence);
  return <>
    {status && <small className="presence-mention-status"><span aria-hidden="true">{statusEmoji(status)} </span>{statusLabel(status)}</small>}
    {shownOnline(presence) && <span className="presence-hidden">, online</span>}
  </>;
}

/** The hidden words that make a row's accessible name read "Meera Iyer, online, In a meeting". */
export function PresenceWords({ id }: { id: string }) {
  const words = presenceWords(usePresence(id));
  return words ? <span className="presence-hidden">, {words}</span> : null;
}

/** "● Online · 🗓️ In a meeting · until 11:30 AM", "Last seen 9:58 AM", or the fallback. */
export function PresenceLine({ id, fallback }: { id: string; fallback: string }) {
  const presence = usePresence(id);
  const status = activeStatus(presence);
  const online = shownOnline(presence);
  const seen = !online && status?.kind !== 'away' && presence?.last_seen ? lastSeenLabel(presence.last_seen) : '';
  if (!online && !status && !seen) return <small className="chat-kind">{fallback}</small>;
  const ends = status ? clearsLabel(status) : '';
  const parts = [
    online && <span key="online" className="presence-online"><span className="presence-dot inline" aria-hidden="true" />Online</span>,
    seen && <span key="seen">{seen}</span>,
    status && <span key="status"><span aria-hidden="true">{statusEmoji(status)} </span>{statusLabel(status)}</span>,
    ends && <span key="ends">{ends}</span>,
  ].filter(Boolean);
  return <small className="chat-kind presence-line">
    {parts.map((part, index) => <Fragment key={index}>{index > 0 && <> <span className="presence-separator" aria-hidden="true">·</span> </>}{part}</Fragment>)}
  </small>;
}
