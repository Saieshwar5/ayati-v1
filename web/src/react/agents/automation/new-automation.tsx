import { Button, Dialog } from '../../ui';

/** "New automation" step 1 (designs/automations-simplified 2): when should the agent
 * start — at a time, or when something happens? Work at a time is asked for in Chat
 * and becomes a task; a trigger opens its form. */
export function NewAutomation({ agent, ask, close, choose }: {
  agent: string; ask?: () => void; close: () => void; choose: () => void;
}) {
  return <Dialog title={`New automation for ${agent}`} close={close} footer={<Button onClick={close}>Cancel</Button>}>
    <p>When should {agent} start this work?</p>
    <div className="automation-kinds">
      <button type="button" className="automation-kind" disabled={!ask} onClick={ask}>
        <strong>At a time</strong>
        <small>Once, or repeating: ask {agent} in Chat and it becomes a task.</small>
        <small className="automation-kind-example">“Every morning at 9, check stock.”</small>
      </button>
      <button type="button" className="automation-kind" onClick={choose}>
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
