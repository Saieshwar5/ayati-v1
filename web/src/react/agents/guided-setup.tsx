import { useCallback, useEffect, useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { workspaceUrl } from '../../workspace-context';
import type { Agent } from './model';
import { activateDraft, createWorkConversation, loadSetup, previewDraft, saveDraft, startReview, type Choice, type Draft, type Preview, type Setup } from './setup-model';
import { SetupReview, Coverage } from './setup-review';
import { SetupPreview } from './setup-preview';
import './guided-setup.css';

type Props = { agent: Agent; close: () => void; changed?: () => void };
export function GuidedSetup(props: Props) {
  const load = useCallback(() => loadSetup(props.agent.id), [props.agent.id]);
  const resource = useResource(load);
  if (resource.data) return <SetupFlow {...props} setup={resource.data} />;
  return <Dialog drawer title={`${props.agent.name} · Set up permissions`} close={props.close} status={resource.error}>
    {!resource.error && <p role="status">Loading eligible accounts and saved review…</p>}
    {resource.error && <Button onClick={() => void resource.refresh().catch(() => {})}>Retry setup</Button>}
  </Dialog>;
}
function SetupFlow({ agent, close, changed, setup }: Props & { setup: Setup }) {
  const [record, setRecord] = useState<Draft | null>(setup.draft);
  const [choices, setChoices] = useState<Choice[]>(setup.draft?.data.choices ?? []);
  const [preview, setPreview] = useState<Preview>();
  const [reviewed, confirm] = useState(false);
  const [step, setStep] = useState(setup.draft ? 1 : 0);
  const [form, setForm] = useState({ purpose: setup.draft?.data.request.purpose ?? setup.context.responsibility, chat: setup.draft?.data.request.chat ?? (setup.context.conversations.length === 1 ? setup.context.conversations[0].id : ''), accounts: setup.draft?.data.request.connections ?? [], assistant: false });
  const [conversations, setConversations] = useState(setup.context.conversations);
  const mutation = useMutation();
  const requests = useRef(new Map<string, string>());
  const alive = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const pending = record?.data.suggestion?.state === 'pending';
  const recordId = record?.id;
  // An executor prepares AI suggestions after the draft is saved; reload until they arrive.
  useEffect(() => {
    if (!pending || !recordId) return;
    let stopped = false;
    const timer = window.setInterval(() => {
      void loadSetup(agent.id).then(latest => {
        const draft = latest.draft;
        if (stopped || !alive.current || draft?.id !== recordId || draft.data.suggestion?.state === 'pending') return;
        setRecord(draft); setChoices(draft.data.choices); setPreview(undefined); confirm(false);
      }).catch(() => {});
    }, 1500);
    return () => { stopped = true; window.clearInterval(timer); };
  }, [agent.id, pending, recordId]);
  const identity = (payload: unknown) => { const key = JSON.stringify(payload); const id = requests.current.get(key) ?? crypto.randomUUID(); requests.current.set(key, id); return id; };
  const edit = (index: number, mode: Choice['mode']) => { setChoices(old => old.map((c, i) => i === index ? { ...c, mode } : c)); setPreview(undefined); confirm(false); };
  const selectedAccounts = setup.context.accounts.filter(a => form.accounts.includes(a.id));
  const approvers = [...new Set(selectedAccounts.flatMap(a => a.permissions.filter(p => p.ceiling !== 'block' && p.approver).map(p => p.approver!)))];
  const names = [...new Set(selectedAccounts.flatMap(a => a.permissions.filter(p => p.ceiling !== 'block' && p.approver_name).map(p => p.approver_name!)))];
  const receipt = record?.state === 'activated' ? record.receipt : null;
  async function persist() {
    if (!record) throw new Error('Create a review draft first.');
    if (JSON.stringify(choices) === JSON.stringify(record.data.choices)) return record;
    const saved = await saveDraft(agent.id, record, choices);
    if (alive.current) { setRecord(saved); setPreview(undefined); confirm(false); }
    return saved;
  }
  const footer = receipt ? <Button onClick={() => { requests.current.clear(); setRecord(null); setPreview(undefined); confirm(false); setStep(0); }}>Start a new review</Button>
    : step === 0 ? <Button primary disabled={mutation.busy || !form.purpose.trim() || !form.chat || !form.accounts.length || form.accounts.length > 5} onClick={() => void mutation.run(async () => {
      const input = { purpose: form.purpose, chat: form.chat, connections: form.accounts, assistant: form.assistant };
      const result = await startReview(agent.id, { ...input, id: identity({ ...input, previous_review: record?.id ?? null }) });
      if (alive.current) { setRecord(result); setChoices(result.data.choices); setPreview(undefined); confirm(false); setStep(1); }
    })}>Create review draft</Button>
      : <><Button disabled={mutation.busy} onClick={() => setStep(step - 1)}>Back</Button>
        {step === 1 ? <><Button disabled={mutation.busy || pending} onClick={() => void mutation.run(async () => { await persist(); if (alive.current) mutation.setStatus('Draft saved. Active permissions have not changed.'); })}>Save draft</Button>
          <Button primary disabled={mutation.busy || pending} onClick={() => void mutation.run(async () => { const saved = await persist(); const result = await previewDraft(agent.id, saved); if (alive.current) { setPreview(result); setStep(2); } })}>Preview permissions</Button></>
          : <Button primary disabled={mutation.busy || !reviewed || !preview || preview.revision !== record?.revision} onClick={() => void mutation.run(async () => {
            const result = await activateDraft(agent.id, record!);
            if (alive.current) { setRecord(old => old && ({ ...old, state: 'activated', receipt: result })); changed?.(); }
          })}>Activate reviewed permissions</Button>}</>;
  return <Dialog drawer title={`${agent.name} · Set up permissions`} close={close} busy={mutation.busy} status={mutation.status || (mutation.busy ? 'Preparing your review…' : '')} footer={footer}>
    <div className="guided-setup">
      {receipt ? <section aria-label="Activation receipt"><h3>Reviewed permissions activated</h3><p>{receipt.operations} actions saved together. Approval requests use the people shown in your review.</p><p className="ui-help">Receipt: {receipt.draft} · {new Date(receipt.at * 1000).toLocaleString()}. Later access changes are checked during execution.</p></section> : <>
        <p className="ui-help">Step {step + 1} of 3 · {['Work and accounts', 'Review actions', 'Preview and activate'][step]}</p>
        {!setup.execution_available && <p role="status">Business tools require controller-managed execution. Blocking actions remains available.</p>}
        {step === 0 && <>
          <label className="ui-field">What should this agent do?<textarea rows={4} maxLength={4000} value={form.purpose} onChange={e => setForm(old => ({ ...old, purpose: e.target.value }))} /></label>
          <section className="setup-accounts" aria-label="Eligible accounts"><h3>Choose approved accounts</h3>
            {setup.context.accounts.map(account => <label key={account.id}><input type="checkbox" aria-label={`Use ${account.name}`} checked={form.accounts.includes(account.id)} onChange={e => setForm(old => ({ ...old, accounts: e.target.checked ? [...old.accounts, account.id] : old.accounts.filter(id => id !== account.id) }))} /><span>{account.name}<small>{account.account || 'Configured business account'}</small></span></label>)}
            {!setup.context.accounts.length && <p>No eligible accounts. An Owner can approve and delegate an account in <a href={workspaceUrl('/?view=settings&scope=workspace&section=connections')}>Workspace Connections</a>.</p>}
            <p className="ui-help">Review up to five accounts together. Other saved account permissions are managed separately.</p>
          </section>
          <label className="ui-field">Work conversation<select aria-label="Work conversation" value={form.chat} onChange={e => setForm(old => ({ ...old, chat: e.target.value }))}><option value="">Choose a conversation</option>{conversations.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}</select></label>
          <p className="ui-help">Ask requests must be visible to the designated approver. {names.length > 0 && `For the selected accounts: ${names.join(', ')}.`}</p>
          <p className="ui-help">Create a new conversation with you{agent.scope === 'workspace' && names.length ? ` and ${names.join(', ')}` : ''}. It starts empty.</p>
          <Button disabled={mutation.busy || !form.accounts.length} onClick={() => void mutation.run(async () => {
            const users = agent.scope === 'workspace' ? approvers : [];
            const name = `${agent.name} · Work`;
            const result = await createWorkConversation(agent.id, name, users, identity({ conversation: name, users }));
            if (alive.current) { setConversations(old => old.some(c => c.id === result.id) ? old : [...old, { id: result.id, name }]); setForm(old => ({ ...old, chat: result.id })); }
          })}>Create work conversation</Button>
          <label className="ui-check"><input type="checkbox" checked={form.assistant} onChange={e => setForm(old => ({ ...old, assistant: e.target.checked }))} />Use AI suggestions for this review</label>
          <p className="ui-help">Sends your description and reviewed action names to this agent’s configured model on a shared executor. Normal model usage may apply. Suggestions arrive after the draft is saved, are editable and cannot activate permissions. Manual review starts with every action blocked.</p>
        </>}
        {step === 1 && record?.data.suggestion && <p role="status" className="ui-help">{record.data.suggestion.detail}</p>}
        {step === 1 && record && !pending && <SetupReview draft={record} choices={choices} change={edit} route={(index, patch) => { setChoices(old => old.map((c, i) => i === index ? { ...c, ...patch } : c)); setPreview(undefined); confirm(false); }} executionAvailable={setup.execution_available} />}
        {step === 2 && record && preview && <><SetupPreview draft={record} preview={preview} busy={mutation.busy} run={example => void mutation.run(async () => { const result = await previewDraft(agent.id, record, example); if (alive.current) setPreview(result); })} />
          <Coverage draft={record} /><label className="ui-check"><input type="checkbox" checked={reviewed} onChange={e => confirm(e.target.checked)} />I reviewed these actions, their limits and designated approvers.</label>
        </>}
      </>}
    </div>
  </Dialog>;
}
