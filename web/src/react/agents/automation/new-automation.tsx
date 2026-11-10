import { Button, Dialog } from '../../ui';

export type AutomationKind = 'time' | 'event';

/** "New automation" step 1 (designs/automations-simplified 2): when should the agent
 * start — at a time, or when something happens? Each choice opens its own form. */
export function NewAutomation({ agent, schedules, close, choose }: {
  agent: string; schedules: boolean; close: () => void; choose: (kind: AutomationKind) => void;
}) {
  return <Dialog title={`New automation for ${agent}`} close={close} footer={<Button onClick={close}>Cancel</Button>}>
    <p>When should {agent} start this work?</p>
    <div className="automation-kinds">
      <button type="button" className="automation-kind" disabled={!schedules} onClick={() => choose('time')}>
        <strong>At a time</strong>
        <small>Once, or repeating: every day, every week, or every few hours.</small>
        <small className="automation-kind-example">“Every morning, check stock.”</small>
        {!schedules && <small>Schedules can’t run on this server.</small>}
      </button>
      <button type="button" className="automation-kind" onClick={() => choose('event')}>
        <strong>When something happens</strong>
        <small>In a connected app: a new email, a new order, a record added or changed.</small>
        <small className="automation-kind-example">“When an order arrives, check stock.”</small>
      </button>
    </div>
  </Dialog>;
}

/** For "When something happens" when both kinds of trigger can be made. */
export function TriggerKind({ close, choose }: { close: () => void; choose: (kind: 'app' | 'create') => void }) {
  return <Dialog title="When something happens" close={close} footer={<Button onClick={close}>Cancel</Button>}>
    <div className="automation-kinds">
      <button type="button" className="automation-kind" onClick={() => choose('app')}>
        <strong>In a connected app</strong>
        <small>Gmail, Google Calendar and other apps your admin connected.</small>
      </button>
      <button type="button" className="automation-kind" onClick={() => choose('create')}>
        <strong>From a reviewed event source</strong>
        <small>A source your operator set up and an admin approved.</small>
      </button>
    </div>
  </Dialog>;
}
