// Keyboard shortcuts in Chat, and the list ⌘/ (Ctrl+/) opens. Only shortcuts
// that exist are listed: each one here is handled below or by the composer.
import { useEffect, useLayoutEffect, useRef } from 'react';
import { Dialog } from '../ui';

const apple = () => /Mac|iPhone|iPad/.test(navigator.userAgent);
/** ⌘ on Apple devices, Ctrl elsewhere. */
export const commandKey = () => apple() ? '⌘' : 'Ctrl';
const option = () => apple() ? '⌥' : 'Alt';

const groups = (): { title: string; keys: [string, string[]][] }[] => [
  { title: 'Move around', keys: [
    ['Search conversations, people and messages', [commandKey(), 'K']],
    ['Next unread conversation', [option(), '⇧', '↓']],
    ['Previous unread conversation', [option(), '⇧', '↑']],
    ['Next or previous conversation', [option(), '↓ / ↑']],
  ] },
  { title: 'Write', keys: [
    ['Send', ['Enter']],
    ['New line', ['⇧', 'Enter']],
    ['Edit your last message', ['↑']],
    ['Mention a person or agent', ['@']],
  ] },
  { title: 'Edit a message', keys: [['Save', ['Enter']], ['Cancel', ['Esc']]] },
  { title: 'Anywhere in Chat', keys: [['Show these shortcuts', [commandKey(), '/']], ['Close a panel or dialog', ['Esc']]] },
];

export function ShortcutsDialog({ close }: { close: () => void }) {
  return <Dialog title="Keyboard shortcuts" close={close}>
    <div className="chat-shortcuts">
      {groups().map(group => <section key={group.title} aria-label={group.title}>
        <h3>{group.title}</h3>
        <dl>{group.keys.map(([what, keys]) => <div key={what}><dt>{what}</dt><dd>{keys.map(key => <kbd key={key}>{key}</kbd>)}</dd></div>)}</dl>
      </section>)}
    </div>
  </Dialog>;
}

export type ShortcutActions = {
  search: () => void; shortcuts: () => void;
  /** Open the next (1) or previous (-1) conversation in the list, or only unread ones. */
  move: (step: 1 | -1, unread: boolean) => void;
};

/** One window listener while Chat is open; actions are read when a key is pressed. */
export function useChatShortcuts(actions: ShortcutActions) {
  const latest = useRef(actions);
  useLayoutEffect(() => { latest.current = actions; });
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      const command = event.metaKey || event.ctrlKey;
      if (command && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'k') { event.preventDefault(); latest.current.search(); }
      else if (command && !event.altKey && event.key === '/') { event.preventDefault(); latest.current.shortcuts(); }
      else if (event.altKey && !command && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
        // Not while a dialog is open: its own controls own the arrows.
        if (document.querySelector('dialog[open]')) return;
        event.preventDefault();
        latest.current.move(event.key === 'ArrowDown' ? 1 : -1, event.shiftKey);
      }
    };
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, []);
}
