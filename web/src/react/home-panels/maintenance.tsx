import { useId, useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { requestUpdate, saveAgreement, type PersonalPanel } from './model';

const labels: Record<string,string> = {
  current: 'Up to date', updating: 'Update queued or running', delayed: 'Update delayed',
  failed: 'Update failed · Previous result retained', blocked: 'Update access needs review',
  paused: 'Updates paused', review_required: 'Update agreement needs review',
  awaiting_update: 'Waiting for first update',
};
export function PanelMaintenance({panel, refresh}: {panel: PersonalPanel; refresh: () => Promise<void>}) {
  const [opened, open] = useState(false);
  const m = useMutation();
  const attempt = useRef<{revision: number; id: string}>(undefined);
  const agreement = panel.maintenance;
  if (!agreement) return null;
  return <div className="panel-maintenance">
    {agreement.revision > 0 && <small role="status">{labels[agreement.status] ?? 'Checking update status'}
      {agreement.checked_ms ? ` · Checked ${new Date(agreement.checked_ms).toLocaleTimeString([], {hour: '2-digit', minute: '2-digit'})}` : ' · No maintenance check yet'}
    </small>}
    <div className="panel-maintenance-actions">
      {agreement.enabled && <Button disabled={m.busy || ['updating', 'review_required', 'blocked'].includes(agreement.status) || !agreement.eligible}
        onClick={() => void m.run(async () => {
          if (attempt.current?.revision !== agreement.revision)
            attempt.current = {revision: agreement.revision, id: crypto.randomUUID()};
          await requestUpdate(panel.id, agreement.revision, attempt.current.id);
          attempt.current = undefined;
          await refresh();
        })}>{m.busy ? 'Requesting update…' : 'Update now'}</Button>}
      <Button onClick={() => open(true)}>Panel updates</Button>
    </div>
    {m.status && <small role="alert">{m.status}</small>}
    {opened && <UpdateAgreement key={`${panel.id}.${agreement.revision}.${panel.active_revision}`}
      panel={panel} close={() => open(false)} refresh={refresh} />}
  </div>;
}
function UpdateAgreement({panel, close, refresh}: {
  panel: PersonalPanel; close: () => void; refresh: () => Promise<void>;
}) {
  const agreement = panel.maintenance!;
  const [instructions, setInstructions] = useState(agreement.instructions);
  const [interval, setInterval] = useState(agreement.interval_seconds ? String(agreement.interval_seconds / 60) : '');
  const [age, setAge] = useState(String(agreement.stale_seconds / 60));
  const [follow, setFollow] = useState(agreement.follow_sources ?? true);
  const [gap, setGap] = useState(String((agreement.min_gap_seconds ?? 300) / 60));
  const m = useMutation();
  const fields = useId();
  const attempt = useRef<{key: string; id: string}>(undefined);
  const save = (enabled: boolean, pause = false) => void m.run(async () => {
    const content = {
      revision: agreement.revision, panel_revision: panel.active_revision!, enabled,
      instructions: pause ? agreement.instructions : instructions,
      interval_seconds: pause ? agreement.interval_seconds : interval ? Number(interval) * 60 : null,
      stale_seconds: pause ? agreement.stale_seconds : Number(age) * 60,
      follow_sources: pause ? agreement.follow_sources ?? true : follow,
      min_gap_seconds: pause ? agreement.min_gap_seconds ?? 300 : Number(gap) * 60,
    };
    const key = JSON.stringify(content);
    if (attempt.current?.key !== key) attempt.current = {key, id: crypto.randomUUID()};
    await saveAgreement(panel.id, {...content, request: attempt.current.id});
    await refresh();
    close();
  });
  return <Dialog title="Panel updates" close={close}>
    <p>{panel.active?.definition?.title} · Personal</p>
    <p>Responsible agent: {panel.agent_name ?? 'Agent'}. Updates run in the original conversation.
      This panel and its maintenance results remain personal.</p>
    {!agreement.execution_available && <p role="status">Background agent execution is unavailable.</p>}
    {!agreement.eligible && <p role="status">Updates need the latest reviewed panel, allowed source reads and permission to request its agent.
      Panels without read receipts or with writes cannot enable maintenance yet.</p>}
    <details>
      <summary>Reviewed source reads</summary>
      {agreement.scopes?.map((scope, i) => <div key={i}><p>{scope.operation} · {scope.connection}</p>
        <pre>{JSON.stringify(scope.arguments, null, 2)}</pre></div>)}
    </details>
    <form className="workspace-panel-form" onSubmit={event => {event.preventDefault(); save(true);}}>
      <div className="workspace-panel-field"><label htmlFor={`${fields}-instructions`}>What should the agent check?</label>
        <textarea id={`${fields}-instructions`} required rows={3} maxLength={2000} value={instructions}
          disabled={m.busy} onChange={event => setInstructions(event.target.value)} /></div>
      <div className="workspace-panel-field"><label htmlFor={`${fields}-interval`}>Check every (minutes)</label>
        <input id={`${fields}-interval`} type="number" min="1" max="1440" step="1" placeholder="Manual updates only"
          value={interval} disabled={m.busy} onChange={event => setInterval(event.target.value)} />
        <small>Leave blank for Update now only. Scheduled checks use elapsed intervals.</small></div>
      <label className="workspace-panel-check"><input type="checkbox" checked={follow} disabled={m.busy}
        onChange={event => setFollow(event.target.checked)} /> Also update when its sources change
        <small>After {panel.agent_name ?? 'the agent'} finishes work that used them, or a new event arrives on them.</small></label>
      {follow && <div className="workspace-panel-field"><label htmlFor={`${fields}-gap`}>At most every (minutes)</label>
        <input id={`${fields}-gap`} type="number" min="1" max="1440" step="1" required
          value={gap} disabled={m.busy} onChange={event => setGap(event.target.value)} /></div>}
      <div className="workspace-panel-field"><label htmlFor={`${fields}-age`}>Consider delayed after (minutes)</label>
        <input id={`${fields}-age`} type="number" min={interval || '1'} max="10080" step="1" required
          value={age} disabled={m.busy} onChange={event => setAge(event.target.value)} /></div>
      <p className="ui-help">New snapshots publish automatically within this agreement. Source reads use the exact reviewed filters
        and current permissions. Business writes and changes to this panel’s purpose or sources need separate authority.
        Scheduled checks may use agent inference.</p>
      {agreement.checked_ms && <p>Last successful source read: {new Date(agreement.checked_ms).toLocaleString()}.</p>}
      {agreement.from_source && <p>Numbers refresh straight from the source; {panel.agent_name ?? 'the agent'} is asked only when something meaningful changes.</p>}
      {agreement.last_day && agreement.last_day.updates > 0 && <p>Last 24 hours: {agreement.last_day.updates} {agreement.last_day.updates === 1 ? 'update' : 'updates'}, {agreement.last_day.without_agent} without the agent.</p>}
      {agreement.enabled && agreement.next_ms && <p>Next scheduled check: {new Date(agreement.next_ms).toLocaleString()}.</p>}
      <div className="panel-maintenance-actions">
        <Button primary type="submit" disabled={m.busy || !agreement.eligible || !instructions.trim()}>
          {m.busy ? 'Saving…' : agreement.enabled ? 'Save update agreement' : 'Enable updates'}</Button>
        {agreement.enabled && <Button disabled={m.busy} onClick={() => save(false, true)}>Pause updates</Button>}
      </div>
      {m.status && <p role="alert">{m.status}</p>}
    </form>
  </Dialog>;
}
