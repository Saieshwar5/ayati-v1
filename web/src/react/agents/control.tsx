import { useCallback, useEffect, useRef, useState } from 'react';
import { Button, Dialog, Tabs } from '../ui';
import { useMutation, useResource } from '../requests';
import { workspaceUrl } from '../../workspace-context';
import {
  decideAction, loadControls, modes, saveRule, states,
  type Action, type Controls, type Mode, type Operation, type Rule,
} from './control-model';
import type { Agent } from './model';

export function AgentControl({ agent, close }: { agent: Agent; close: () => void }) {
  const load = useCallback(() => loadControls(agent.id), [agent.id]);
  const { data, error, refresh } = useResource(load);
  const mutation = useMutation();
  const decisions = useRef(new Map<string, string>());
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (!document.hidden) void refresh().catch(() => {});
    }, 5000);
    return () => clearInterval(timer);
  }, [refresh]);

  async function decide(action: Action, decision: 'approve' | 'reject') {
    const key = `${action.id}/${decision}`;
    const id = decisions.current.get(key) ?? crypto.randomUUID();
    decisions.current.set(key, id);
    await decideAction(agent.id, action.id, id, decision);
    await refresh();
  }

  const permissions = data && <section className="control-section" aria-label="Connected systems">
    {data.execution_available === false && <p role="status" className="ui-help">Business tools are not enabled for this workspace’s worker setup. Ask your operator to enable controller-managed execution.</p>}
    <p className="ui-help">Connecting a system gives the agent no access until you save a permission for a conversation.</p>
    <p className="ui-help">Each permission covers the records available to the connected account. Additional per-record, amount and time conditions are not available yet.</p>
    {data.connections.map(connection => <div className="control-connection" key={connection.id}>
      <div className="control-heading">
        <strong>{connection.definition.name}</strong>
        <span>{connection.enabled ? 'Connected' : 'Disconnected'}</span>

      </div>
      {connection.enabled && connection.definition.operations.map(operation => {
        const rule = data.rules.find(r => r.connection === connection.id && r.operation === operation.id);
        const delegation = data.delegations?.find(d => d.connection === connection.id && d.operation === operation.id);
        const ceiling = data.can_manage_connections ? 'allow' : delegation?.ceiling ?? 'block';
        return <PermissionRow
          key={`${connection.id}/${operation.id}/${rule?.revision ?? 0}/${delegation?.revision ?? 0}`}
          conversations={data.conversations}
          operation={operation}
          rule={rule}
          editable={data.can_manage && Boolean(agent.editable) && (data.can_manage_connections || Boolean(delegation))}
          ceiling={ceiling}
          approver={data.can_manage_connections ? undefined : delegation?.approver_name}
          busy={mutation.busy}
          executionAvailable={data.execution_available !== false}
          save={(mode, revision, chat) => mutation.run(async () => {
            await saveRule(agent.id, { connection: connection.id, operation: operation.id, mode, revision, chat, ...(!data.can_manage_connections && delegation ? { delegation_revision: delegation.revision } : {}) });
            await refresh();
          })}
        />;
      })}
    </div>)}
    {!data.connections.length && <p className="ui-help">No connected business systems yet.</p>}
    {data.can_manage_connections && <a href={workspaceUrl('/?view=settings&scope=workspace&section=connections')}>Manage workspace connections and delegated access</a>}
    {!data.can_manage_connections && <p className="ui-help">Only operations delegated by an Owner can be configured here. Ask an Owner if an account or permission is missing.</p>}

  </section>;

  const activity = data && <section className="control-section" aria-label="Action history">
    <p className="ui-help">The latest 50 actions visible to you. Approval applies to the exact fields shown. Unknown outcomes need reconciliation before further writes.</p>
    {!data.actions.length && <p className="ui-help">Assign an approved system in Permissions, then ask this agent to use an assigned operation. Its proposals and results will appear here.</p>}
    {data.actions.map(action => <ActionRow
      key={action.id}
      action={action}
      controls={data}
      busy={mutation.busy}
      decide={decision => mutation.run(() => decide(action, decision))}
    />)}
  </section>;

  return <Dialog drawer title={`${agent.name} · Tools and approvals`} close={close} busy={mutation.busy} status={error || mutation.status}>
    <Button disabled={mutation.busy} onClick={() => void refresh().catch(() => {})}>Refresh activity</Button>
    {!data && !error && <p role="status">Loading tools and approvals…</p>}
    {data && <Tabs sections={[
      { title: 'Activity', content: activity },
      { title: 'Permissions', content: permissions },
    ]} />}
  </Dialog>;
}

type PermissionProps = {
  conversations: Controls['conversations']; operation: Operation; rule?: Rule;
  editable: boolean; busy: boolean; ceiling: Mode; approver?: string;
  executionAvailable: boolean;
  save: (mode: Mode, revision: number, chat: string) => Promise<void>;
};
function PermissionRow({ conversations, operation, rule, editable, busy, ceiling, approver, executionAvailable, save }: PermissionProps) {
  const [mode, setMode] = useState<Mode>(rule?.mode ?? 'block');
  const [chat, setChat] = useState(rule?.chat ?? (conversations.length === 1 ? conversations[0].id : ''));
  return <form className="control-rule" onSubmit={event => {
    event.preventDefault();
    void save(mode, rule?.revision ?? 0, chat);
  }}>
    <div>
      <strong>{operation.title}</strong>
      <small>{operation.effect === 'write' ? 'Changes business records' : 'Reads business records'}</small>
      {rule?.mode === 'ask' && !approver && <small>Approver: {rule.approver_name}</small>}
      {approver && <small>Your limit: {modes[ceiling]}. Ask requests go to {approver}.</small>}
    </div>
    <div className="control-rule-fields">
      <label className="ui-field">Conversation
        <select aria-label={`${operation.title} conversation`} required value={chat} disabled={!editable || busy} onChange={event => setChat(event.target.value)}>
          <option value="">Choose a conversation</option>
          {conversations.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
        </select>
      </label>
      <label className="ui-field">Permission
        <select aria-label={`${operation.title} permission`} value={mode} disabled={!editable || busy} onChange={event => setMode(event.target.value as Mode)}>
          {Object.entries(modes).map(([value, label]) => <option key={value} value={value} disabled={(!executionAvailable && value !== 'block') || (ceiling === 'block' && value !== 'block') || (ceiling === 'ask' && value === 'allow')}>{label}</option>)}
        </select>
      </label>
    </div>
    {editable && <Button type="submit"
      disabled={busy || !chat || (!executionAvailable && mode !== 'block') || (Boolean(rule) && mode === rule?.mode && chat === rule.chat)}
      aria-label={`Save ${operation.title} permission`}>Save permission</Button>}
    {!conversations.length && <p className="ui-help">Start a conversation with this agent before assigning an operation.</p>}
  </form>;
}

function ActionFields({ fields }: { fields: unknown }) {
  if (!fields || typeof fields !== 'object' || Array.isArray(fields)) return null;
  return <dl className="control-fields">
    {Object.entries(fields).map(([field, value]) => <div key={field}>
      <dt>{field.replaceAll('_', ' ')}</dt>
      <dd>{typeof value === 'boolean' ? (value ? 'Yes' : 'No') : String(value)}</dd>
    </div>)}
  </dl>;
}

type ActionProps = {
  action: Action; controls: Controls; busy: boolean;
  decide: (decision: 'approve' | 'reject') => Promise<void>;
};
function ActionRow({ action, controls, busy, decide }: ActionProps) {
  const awaiting = action.state === 'pending';
  const canDecide = awaiting && controls.can_manage && controls.viewer === action.approver;
  const system = controls.connections.find(connection => connection.id === action.connection)?.definition.name ?? 'Previously connected system';
  return <article className="control-action" aria-label={`${action.title} · ${states[action.state] ?? action.state}`}>
    <div className="control-heading">
      <strong>{action.title}</strong>
      <span role="status">{states[action.state] ?? action.state}</span>
    </div>
    <small>{new Date(action.created * 1000).toLocaleString()} · {action.effect === 'write' ? 'Changes records' : 'Reads records'}</small>
    <p className="ui-help">{system} · {action.conversation_name ?? 'Conversation'} · Requested by {action.requester_name ?? 'a workspace member'}</p>
    {action.state === 'completed' && <p className="ui-help">{action.decision === 'approve' ? 'Executed after exact approval.' : 'Executed under the saved automatic permission.'}</p>}
    <details open={awaiting}>
      <summary>Exact action fields</summary>
      <ActionFields fields={action.arguments} />
    </details>
    {canDecide && <div className="control-decisions">
      <Button primary disabled={busy} onClick={() => void decide('approve')}>Approve this action</Button>
      <Button disabled={busy} onClick={() => void decide('reject')}>Reject this action</Button>
    </div>}
    {awaiting && !canDecide && <p className="ui-help">Waiting for the designated approver.</p>}
    {action.state === 'invalidated' && <p className="ui-help">Permissions or connection verification changed. Ask the agent for a new proposal.</p>}
    {action.state === 'uncertain' && <p className="ui-help">The remote system may have changed. Inspect it before requesting another write.</p>}
    {action.state === 'completed' && <details>
      <summary>Confirmed result</summary>
      <pre>{JSON.stringify(action.result, null, 2)}</pre>
    </details>}
    <a href={workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(action.chat)}${action.thread ? `&thread=${encodeURIComponent(action.thread)}` : ''}`)}>Open conversation</a>
    <small>Action {action.id}</small>
  </article>;
}
