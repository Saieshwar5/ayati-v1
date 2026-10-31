import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { currentWorkspace } from '../../workspace-context';
import { useMutation } from '../requests';
import { cancelDeletion, deletionDay, scheduleDeletion, type WorkspaceSettings } from './workspace-model';

/** Danger zone: delete with a 7-day undo; export arrives later (designs/settings 4). */
export function WorkspaceDeletion({ settings, changed }: { settings: WorkspaceSettings; changed: () => Promise<void> }) {
  const [confirming, setConfirming] = useState(false);
  const [typed, setTyped] = useState('');
  const mutation = useMutation();
  // The shell's banner reads the session entry; keep it in step.
  const remember = (at: number | null) => { const entry = currentWorkspace(); if (entry) entry.delete_at_ms = at; };
  const undo = () => void mutation.run(async () => { await cancelDeletion(); remember(null); await changed(); mutation.setStatus('Deletion cancelled. The workspace stays.'); });
  const schedule = () => void mutation.run(async () => {
    const result = await scheduleDeletion(typed); remember(result.delete_at_ms); setConfirming(false); setTyped(''); await changed();
  });
  return <section className="settings-danger" aria-label="Danger zone">
    <div className="settings-danger-row">
      <div><h3>Delete workspace</h3>
        {settings.delete_at_ms
          ? <p role="status"><strong>Scheduled for {deletionDay(settings.delete_at_ms)}</strong>{settings.delete_requested_by ? ` by ${settings.delete_requested_by}` : ''}. Until then everything keeps working and any admin can undo it.</p>
          : <p>Deletes chats, agents, files and settings. You can undo for 7 days; then it is permanent.</p>}</div>
      {settings.delete_at_ms
        ? <Button disabled={mutation.busy} onClick={undo}>Undo deletion</Button>
        : <Button className="danger" onClick={() => setConfirming(true)}>Delete workspace…</Button>}
    </div>
    <div className="settings-danger-row"><div><h3>Export data</h3><p>Download everything in the workspace.</p></div>
      <span className="ui-badge settings-soon">Coming later</span></div>
    {mutation.status && !confirming && <p className="ui-status" role="status">{mutation.status}</p>}
    {confirming && <Dialog title="Delete this workspace?" busy={mutation.busy} status={mutation.status} close={() => { setConfirming(false); setTyped(''); }}
      footer={<><Button onClick={() => { setConfirming(false); setTyped(''); }}>Cancel</Button>
        <Button className="danger" disabled={mutation.busy || typed.trim() !== settings.name} onClick={schedule}>Delete in 7 days</Button></>}>
      <p>Everyone loses access to <strong>{settings.name}</strong> after 7 days: its chats, agents, schedules, files and settings. Until then any admin can undo it.</p>
      <label className="ui-field">Type the workspace name to confirm<input value={typed} onChange={event => setTyped(event.target.value)} autoComplete="off" /></label>
    </Dialog>}
  </section>;
}
