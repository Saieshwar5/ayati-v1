import { Button } from '../ui';

/** Shown only while a form has unsaved text. Typed fields save from here; switches
 * and choices elsewhere in Settings save the moment they change. */
export function SaveBar({ form, busy, disabled = false, discard }: { form: string; busy: boolean; disabled?: boolean; discard: () => void }) {
  return <div className="settings-save-bar" role="group" aria-label="Unsaved changes">
    <span>Unsaved changes</span>
    <span className="settings-save-bar-actions">
      <Button disabled={busy} onClick={discard}>Discard</Button>
      <Button primary type="submit" form={form} disabled={busy || disabled}>{busy ? 'Saving…' : 'Save'}</Button>
    </span>
  </div>;
}
