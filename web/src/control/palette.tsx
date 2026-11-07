import { useEffect, useMemo, useRef, useState } from 'react';
import type { Go } from './app';

type Command = { label: string; cli: string; run: (go: Go, text: string) => void };

const COMMANDS: Command[] = [
  { label: 'Open the overview', cli: 'ayati admin overview', run: go => go('overview') },
  { label: 'Turn a provider off or on', cli: 'ayati admin provider off NAME --reason …', run: go => go('providers') },
  { label: 'Keep a provider’s API key here', cli: 'ayati admin provider key NAME --reason …', run: go => go('providers') },
  { label: 'See catalog routes and margins', cli: 'ayati admin models', run: go => go('routing') },
  { label: 'Issue an invite code', cli: 'ayati admin code issue --for "…"', run: go => go('codes') },
  { label: 'See this month’s spend', cli: 'ayati admin spend', run: go => go('spend') },
  { label: 'Change a platform setting', cli: 'ayati admin setting set KEY VALUE --reason …', run: go => go('config') },
  { label: 'Read the change log', cli: 'ayati admin log', run: go => go('log') },
  { label: 'Read problem reports', cli: 'ayati admin reports', run: go => go('reports') },
];

/** ⌘K: jump anywhere, or search workspaces by name, email or id. */
export function CommandPalette({ go, close }: { go: Go; close: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [text, setText] = useState('');
  useEffect(() => {
    const opener = document.activeElement;
    const node = dialog.current;
    node?.showModal();
    return () => {
      if (node?.open) node.close();
      (opener as HTMLElement | null)?.focus?.();
    };
  }, []);
  const matches = useMemo(() => {
    const words = text.trim().toLowerCase();
    const found = COMMANDS.filter(c => !words || c.label.toLowerCase().includes(words) || c.cli.includes(words));
    const search: Command[] = words ? [{ label: `Find workspaces matching “${text.trim()}”`, cli: `ayati admin workspaces ${text.trim()}`,
      run: (navigate, query) => navigate('workspaces', { q: query.trim() }) }] : [];
    return [...search, ...found];
  }, [text]);
  return <dialog ref={dialog} className="cc-dialog cc-palette" aria-label="Command palette"
    onCancel={event => { event.preventDefault(); close(); }}
    onClick={event => { if (event.target === dialog.current) close(); }}>
    <form onSubmit={event => { event.preventDefault(); matches[0]?.run(go, text); }}>
      <label className="cc-visually-hidden" htmlFor="cc-command">Command or workspace</label>
      <input id="cc-command" autoFocus autoComplete="off" placeholder="Type a command or a workspace name" value={text}
        onChange={event => setText(event.target.value)} />
    </form>
    <ul role="list">
      {matches.map(command => <li key={command.label}>
        <button type="button" onClick={() => command.run(go, text)}><span>{command.label}</span><code>{command.cli}</code></button>
      </li>)}
    </ul>
    <p className="cc-dim cc-palette-foot">Every command here is also an <code>ayati admin</code> command in the terminal.</p>
  </dialog>;
}
