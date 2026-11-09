import { useEffect, useRef, useState, type Ref } from 'react';
import { useMutation } from '../requests';
import { ModePicker } from '../agents/mode-picker';
import { modes, type Mode } from '../agents/control-model';
import { meanings, saveConnectorRule, saveMeaning, type Meaning, type Safety } from '../settings/workspace-connections-model';
import type { WorkspaceApp } from './apps-model';

type Operation = Safety['operations'][number];

/** What agents may do with each action of one service (designs/connector-screens 1).
 * Admins change each rule and kind right here; everyone else reads them. The rule is
 * the loosest any agent may use: agent owners can only make it stricter. */
export function ServiceRules({ app, safety, changed, heading }: {
  app: WorkspaceApp; safety?: Safety; changed: () => Promise<void>; heading?: Ref<HTMLHeadingElement>;
}) {
  const mutation = useMutation();
  // A refused save (often a stale revision) reloads the rules, so the next try is current.
  const run = (save: () => Promise<unknown>, done: string) => void mutation.run(async () => {
    try { await save(); } catch (failure) { await changed().catch(() => {}); throw failure; }
    await changed();
    mutation.setStatus(done);
  });
  const rule = (operation: Operation, mode: Mode, needsOther: boolean) => run(() => saveConnectorRule({ connection: safety!.connection, operation: operation.id, mode,
    needs_other: mode === 'ask' && needsOther, revision: operation.revision }), `${operation.title}: ${modes[mode]}${mode === 'ask' && needsOther ? ', someone else approves' : ''}.`);
  const kind = (operation: Operation, meaning: Meaning) => run(() => saveMeaning({ connection: safety!.connection, operation: operation.id, meaning,
    revision: operation.meaning_revision }), `${operation.title}: ${meanings[meaning]}.`);
  return <section className="service-section" aria-label="What agents may do">
    <h3 ref={heading} tabIndex={-1}>What agents may do{safety && <span>You can change these</span>}</h3>
    {safety ? <ul className="service-rules">{safety.operations.map(operation => <li key={operation.id}>
      <span className="service-rule-text"><strong>{operation.title}</strong>
        <KindMenu operation={operation} disabled={mutation.busy} choose={meaning => kind(operation, meaning)}
          other={on => rule(operation, 'ask', on)} /></span>
      <ModePicker label={`${operation.title} workspace rule`} value={operation.mode} disabled={mutation.busy}
        change={mode => rule(operation, mode, operation.needs_other)} />
    </li>)}</ul>
      : <ul className="app-actions">{app.actions.map(action => <li key={action.id}>
        <span>{action.title}<small>{meanings[action.meaning]}</small></span><span className={`app-mode ${action.mode}`}>{modes[action.mode]}</span></li>)}</ul>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
  </section>;
}

/** The kind under an action's name (▾): correct what it does, and for Ask first,
 * require someone other than the requester to approve. */
function KindMenu({ operation, disabled, choose, other }: {
  operation: Operation; disabled: boolean; choose: (meaning: Meaning) => void; other: (on: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null), menu = useRef<HTMLDivElement>(null);
  // Reading actions can only read; the menu is still needed to require a second approver.
  const kinds = (Object.keys(meanings) as Meaning[]).filter(meaning => (meaning === 'read') === (operation.effect === 'read'));
  const asks = operation.mode === 'ask';
  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLElement>('[role^=menuitem]')?.focus();
    const outside = (event: MouseEvent) => { if (!menu.current?.contains(event.target as Node) && !toggle.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, [open]);
  const pick = (act: () => void) => { setOpen(false); toggle.current?.focus(); act(); };
  const notes = [asks && operation.needs_other && 'someone else approves', !operation.meaning_reviewed && 'suggested'].filter(Boolean).join(' · ');
  return <span className="service-kind" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false); }}
    onKeyDown={event => {
      if (event.key === 'Escape' && open) { event.preventDefault(); event.stopPropagation(); setOpen(false); toggle.current?.focus(); }
      if (open && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
        event.preventDefault();
        const items = [...(menu.current?.querySelectorAll<HTMLElement>('[role^=menuitem]') ?? [])];
        const at = items.indexOf(document.activeElement as HTMLElement);
        items[(at + (event.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length]?.focus();
      }
    }}>
    {kinds.length > 1 || asks
      ? <button ref={toggle} type="button" className="service-kind-button" aria-haspopup="menu" aria-expanded={open} disabled={disabled}
        aria-label={`What ${operation.title} does: ${meanings[operation.meaning]}`} onClick={() => setOpen(!open)}>{meanings[operation.meaning]} ▾</button>
      : <span>{meanings[operation.meaning]}</span>}
    {notes && <span> · {notes}</span>}
    {open && <div ref={menu} className="service-menu" role="menu" aria-label={`What ${operation.title} does`}>
      {kinds.map(meaning => <button key={meaning} type="button" role="menuitemradio" aria-checked={operation.meaning === meaning}
        onClick={() => pick(() => choose(meaning))}>{meanings[meaning]}</button>)}
      {asks && <button type="button" role="menuitemcheckbox" aria-checked={operation.needs_other} className="service-menu-other"
        onClick={() => pick(() => other(!operation.needs_other))}>Someone other than the requester approves</button>}
    </div>}
  </span>;
}
