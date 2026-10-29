import { useState } from 'react';
import { useMutation } from '../requests';
import { Button } from '../ui';
import { chatRequest, type Conversation } from './model';
import { alertsSupported, defaultLevel, dismissPrompt, promptDismissed, rememberLevel, type Level } from './notifications';

const CHOICES: { level: Level; label: string; help: (direct: boolean) => string }[] = [
  { level: 'all', label: 'All new messages', help: direct => direct ? 'Default for direct messages' : 'Every message here' },
  { level: 'mentions', label: '@mentions and replies', help: direct => direct ? 'Only when you are mentioned' : 'Default for channels' },
  { level: 'none', label: 'Nothing', help: () => 'Muted · the unread count still shows' },
];

/** The bell in a conversation header: how much this conversation may alert you. */
export function NotifyMenu({ chat, level, changed }: { chat: Conversation; level?: Level; changed: (level: Level) => void }) {
  const [open, setOpen] = useState(false);
  const mutation = useMutation();
  const current = level ?? defaultLevel(chat);
  const choose = (next: Level) => void mutation.run(async () => {
    await chatRequest(`/chats/${chat.id}/notifications`, { level: next });
    rememberLevel(chat.id, next); changed(next); setOpen(false);
  });
  return <span className="chat-notify">
    <Button className="ui-icon chat-notify-button" aria-label={`Notifications: ${CHOICES.find(choice => choice.level === current)!.label}`} aria-expanded={open}
      title="Notifications" onClick={() => setOpen(!open)}><BellIcon muted={current === 'none'} /></Button>
    {open && <div className="chat-notify-menu" role="menu" aria-label={`Notify me in ${chat.kind === 'direct' ? 'this conversation' : `#${chat.name}`} about`}>
      {CHOICES.map(choice => <button type="button" role="menuitemradio" aria-checked={current === choice.level} key={choice.level} disabled={mutation.busy} onClick={() => choose(choice.level)}>
        <span>{choice.label}<small>{choice.help(chat.kind === 'direct')}</small></span>{current === choice.level && <span aria-hidden="true">✓</span>}
      </button>)}
      {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
    </div>}
  </span>;
}

/** A one-time offer at the top of the chat list to allow desktop alerts. */
export function AlertsPrompt() {
  const [hidden, setHidden] = useState(() => !alertsSupported() || Notification.permission !== 'default' || promptDismissed());
  if (hidden) return null;
  return <div className="chat-alerts-prompt" role="region" aria-label="Desktop notifications">
    <strong><BellIcon /> Get notified about messages</strong>
    <p>Desktop alerts for direct messages, @mentions and replies to your requests.</p>
    <div>
      <Button primary onClick={() => void Notification.requestPermission().finally(() => setHidden(true))}>Turn on</Button>
      <Button onClick={() => { dismissPrompt(); setHidden(true); }}>Not now</Button>
    </div>
  </div>;
}

export function BellIcon({ muted = false }: { muted?: boolean }) {
  return <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true">
    {muted ? <><path d="M6 9a6 6 0 0 1 9.5-4.9M18 9c0 6 2 7 2 7H8" /><path d="M10 20a2 2 0 0 0 4 0M3 3l18 18" /></>
      : <><path d="M6 9a6 6 0 0 1 12 0c0 6 2 7 2 7H4s2-1 2-7" /><path d="M10 20a2 2 0 0 0 4 0" /></>}
  </svg>;
}
