// Set a status (designs/chat-and-account screen 3, D245): one of the presets or
// your own words with an emoji, and when it clears. Opened from the status line
// in the account menu. The server checks the text and the emoji again.
import { useId, useRef, useState, type ReactNode } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { EmojiPicker } from './emoji-picker';
import { chatRequest } from './model';
import { activeStatus, applyPresence, ownPresence, type Presence, type Status, type StatusKind } from './presence';
import { presets, statusEmoji } from './presence-view';

type Choice = 'available' | StatusKind;
export type ClearAfter = 'hour' | 'today' | 'week' | 'never';
const clearChoices: { value: ClearAfter; label: string }[] = [
  { value: 'hour', label: '1 hour' }, { value: 'today', label: 'Today' }, { value: 'week', label: 'This week' }, { value: 'never', label: 'Never' },
];
/** The server's limit for your own words (src/identity/chat/status.rs). */
const TEXT_LIMIT = 80;

/** When a status chosen at `now` clears, in your own time zone: in an hour, at
 * midnight, or at the end of Sunday. None for Never. */
export function clearAt(after: ClearAfter, now = new Date()): number | null {
  if (after === 'never') return null;
  if (after === 'hour') return now.getTime() + 3_600_000;
  const days = after === 'today' ? 1 : 8 - (now.getDay() || 7);
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() + days).getTime();
}

function clearHelp(after: ClearAfter, now = new Date()): string {
  if (after === 'never') return 'Stays until you change or clear it.';
  const when = after === 'hour' ? `at ${new Date(clearAt(after, now)!).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })}`
    : after === 'today' ? 'tonight at midnight' : 'at the end of Sunday';
  return `Clears ${when}, then you’re Available again.`;
}

/** The request for what is chosen; null clears the status (Available). */
export function chosenStatus(choice: Choice, text: string, emoji: string, after: ClearAfter, now = new Date()): Status | null {
  const words = text.trim();
  if (words) return { kind: 'custom', emoji: emoji || null, text: words, clear_at: clearAt(after, now) };
  if (choice === 'available' || choice === 'custom') return null;
  return { kind: choice, emoji: emoji || null, text: null, clear_at: clearAt(after, now) };
}

export function StatusDialog({ close }: { close: () => void }) {
  const current = activeStatus(ownPresence());
  const [choice, setChoice] = useState<Choice>(current?.kind ?? 'available');
  const [text, setText] = useState(current?.kind === 'custom' ? current.text ?? '' : '');
  const [emoji, setEmoji] = useState(current?.emoji ?? '');
  const [after, setAfter] = useState<ClearAfter>(current && current.clear_at == null ? 'never' : 'today');
  const mutation = useMutation();
  const input = useRef<HTMLInputElement>(null);
  const name = useId();
  const typed = text.trim() !== '';
  const preset = presets.find(item => item.kind === choice && !typed);
  const save = (status: Status | null) => void mutation.run(async () => {
    applyPresence([await chatRequest<Presence>('/status', { status })]);
    close();
  });
  const pick = (next: Choice) => { setChoice(next); setText(''); setEmoji(''); };
  return <Dialog title="Set a status" busy={mutation.busy} status={mutation.status} close={close} initialFocus={input}
    footer={<div className="status-dialog-actions">
      {current && <Button className="status-dialog-clear" disabled={mutation.busy} onClick={() => save(null)}>Clear status</Button>}
      <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
      <Button primary disabled={mutation.busy} onClick={() => save(chosenStatus(choice, text, emoji, after))}>{mutation.busy ? 'Saving…' : 'Save'}</Button>
    </div>}>
    <form className="status-dialog" onSubmit={event => { event.preventDefault(); save(chosenStatus(choice, text, emoji, after)); }}>
      <div className="status-dialog-own">
        <EmojiPicker chosen={emoji || (preset ? preset.emoji : '')} insert={value => { setEmoji(value); input.current?.focus(); }} />
        <input ref={input} type="text" aria-label="Your own status" placeholder="Add your own: “Out on deliveries”" maxLength={TEXT_LIMIT} value={text}
          onChange={event => { setText(event.target.value); if (event.target.value.trim()) setChoice('custom'); }} />
      </div>
      <fieldset className="status-dialog-choices">
        <legend className="presence-hidden">Status</legend>
        <StatusChoice name={name} value="available" choice={choice} typed={typed} pick={pick} label="Available" help="Shows when you have Cuckoo open."
          mark={<span className="presence-dot inline" aria-hidden="true" />} />
        {presets.map(item => <StatusChoice key={item.kind} name={name} value={item.kind} choice={choice} typed={typed} pick={pick}
          label={item.label} help={item.help} mark={<span aria-hidden="true">{statusEmoji({ kind: item.kind })}</span>} />)}
      </fieldset>
      <fieldset className="status-dialog-after" disabled={!typed && choice === 'available'}>
        <legend>Clear after</legend>
        <div className="status-dialog-segments">{clearChoices.map(item => <label key={item.value} className={after === item.value ? 'chosen' : undefined}>
          <input type="radio" name={`${name}-after`} value={item.value} checked={after === item.value} onChange={() => setAfter(item.value)} />{item.label}
        </label>)}</div>
        <p className="ui-help">{clearHelp(after)}</p>
      </fieldset>
    </form>
  </Dialog>;
}

function StatusChoice({ name, value, choice, typed, pick, label, help, mark }: {
  name: string; value: Choice; choice: Choice; typed: boolean; pick: (choice: Choice) => void;
  label: string; help: string; mark: ReactNode;
}) {
  const checked = !typed && choice === value;
  return <label className={`status-dialog-choice${checked ? ' chosen' : ''}`}>
    <span className="status-dialog-mark">{mark}</span>
    <span><strong>{label}</strong><small>{help}</small></span>
    <input type="radio" name={name} value={value} checked={checked} onChange={() => pick(value)} />
  </label>;
}
