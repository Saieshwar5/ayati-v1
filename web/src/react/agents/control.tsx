import { useCallback, useEffect, useRef, useState } from 'react';
import { Button, Dialog, Tabs } from '../ui';
import { useMutation, useResource } from '../requests';
import { workspaceUrl } from '../../workspace-context';
import {
  connectServer, decideAction, loadControls, modes, saveRule, states,
  type Action, type Controls, type Mode, type Operation, type Rule,
} from './control-model';
import type { Agent } from './model';

export function AgentControl({ agent, close }: { agent: Agent; close: () => void }) {
  const load = useCallback(() => loadControls(agent.id), [agent.id]);
  const { data, error, refresh } = useResource(load);
  const mutation = useMutation();
  const [draft, setDraft] = useState({ id: crypto.randomUUID(), server: '' });
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
        {data.can_manage && <Button disabled={mutation.busy || (!connection.enabled && data.execution_available === false)} onClick={() => void mutation.run(async () => {
          await connectServer(agent.id, { id: connection.id, server: connection.server, enabled: !connection.enabled });
          await refresh();
        })}>{connection.enabled ? 'Disconnect' : 'Reconnect and review'}</Button>}
      </div>
      {connection.enabled && connection.definition.operations.map(operation => {
        const rule = data.rules.find(r => r.connection === connection.id && r.operation === operation.id);
        return <PermissionRow
          key={`${connection.id}/${operation.id}/${rule?.revision ?? 0}`}
          conversations={data.conversations}
          operation={operation}
          rule={rule}
          editable={data.can_manage && Boolean(agent.editable)}
          busy={mutation.busy}
          executionAvailable={data.execution_available !== false}
          save={(mode, revision, chat) => mutation.run(async () => {
            await saveRule(agent.id, { connection: connection.id, operation: operation.id, mode, revision, chat });
            await refresh();
          })}
        />;
      })}
    </div>)}
    {!data.connections.length && <p className="ui-help">No connected business systems yet.</p>}
    {data.can_manage && Boolean(agent.editable) && data.execution_available !== false && <form className="ui-form" onSubmit={event => {
      event.preventDefault();
      void mutation.run(async () => {
        await connectServer(agent.id, { ...draft, enabled: true });
        setDraft({ id: crypto.randomUUID(), server: '' });
        await refresh();
      });
    }}>
      <label className="ui-field">Available business system
        <select value={draft.server} disabled={mutation.busy} onChange={event => {
          setDraft({ id: crypto.randomUUID(), server: event.target.value });
        }}>
          <option value="">Choose a system</option>
          {data.servers.filter(server => !data.connections.some(c => c.server === server.id)).map(server =>
            <option key={server.id} value={server.id}>{server.name}</option>)}
        </select>
      </label>
      <Button type="submit" primary disabled={mutation.busy || !draft.server}>Connect system</Button>
      {!data.servers.length && <p className="ui-help">Your operator needs to configure a supported remote MCP connection. Credentials are never entered in agent chat.</p>}
    </form>}
    {!data.can_manage && <p className="ui-help">A workspace owner manages business tool permissions.</p>}
  </section>;

  const activity = data && <section className="control-section" aria-label="Action history">
    <p className="ui-help">The latest 50 actions visible to you. Approval applies to the exact fields shown. Unknown outcomes need reconciliation before further writes.</p>
    {!data.actions.length && <p className="ui-help">Connect a system in Permissions, then ask this agent to use an assigned operation. Its proposals and results will appear here.</p>}
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
  editable: boolean; busy: boolean;
  executionAvailable: boolean;
  save: (mode: Mode, revision: number, chat: string) => Promise<void>;
};
function PermissionRow({ conversations, operation, rule, editable, busy, executionAvailable, save }: PermissionProps) {
  const [mode, setMode] = useState<Mode>(rule?.mode ?? 'block');
  const [chat, setChat] = useState(rule?.chat ?? (conversations.length === 1 ? conversations[0].id : ''));
  return <form className="control-rule" onSubmit={event => {
    event.preventDefault();
    void save(mode, rule?.revision ?? 0, chat);
  }}>
    <div>
      <strong>{operation.title}</strong>
      <small>{operation.effect === 'write' ? 'Changes business records' : 'Reads business records'}</small>
      {rule?.mode === 'ask' && <small>Approver: {rule.approver_name}</small>}
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
          {Object.entries(modes).map(([value, label]) => <option key={value} value={value} disabled={!executionAvailable && value !== 'block'}>{label}</option>)}
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
