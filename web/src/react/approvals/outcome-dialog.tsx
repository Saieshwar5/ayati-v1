import { useCallback, useId, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { states } from '../agents/control-model';
import { clockTime, dayName } from '../apps/apps-model';
import { PauseIcon } from '../apps/app-marks';
import { fieldEntries } from './fields';
import { loadApproval, settleOutcome, type Approval, type Settle } from './model';
import './approvals.css';

const checked: Record<Settle['checks'][number]['result'], (app: string) => string> = {
  unknown: app => `Looked for it: ${app} didn’t answer`,
  happened: app => `Found it in ${app}`,
  not_happened: app => `Looked: ${app} has no record of it`,
};

/** "Did this go out?" (designs/connector-screens 4): a write the app never confirmed
 * pauses that account's writes until one of the agent's managers or an admin says
 * whether it happened. Opened from Apps and from Activity's Needs review. */
export function OutcomeDialog({ id, close }: { id: string; close: () => void }) {
  const load = useCallback(() => loadApproval(id), [id]);
  const { data, error, refresh, setData } = useResource(load);
  const mutation = useMutation();
  const [happened, setHappened] = useState<boolean>();
  const [note, setNote] = useState('');
  const name = useId();
  const app = data?.account_name || 'The app';
  const waiting = data?.state === 'uncertain' && !data.settled;
  const canAnswer = waiting && Boolean(data?.settle?.can_settle);
  const save = () => void mutation.run(async () => {
    if (happened === undefined) return;
    try {
      const saved = await settleOutcome(id, { happened, note: note.trim() });
      setData(old => old && { ...old, ...saved });
      mutation.setStatus(`Answer saved. ${app} writes can run again.`);
    } catch (failure) {
      if (failure && typeof failure === 'object' && 'status' in failure && failure.status === 409) {
        await refresh().catch(() => {});
        throw new Error('Someone else answered first, so your answer was not saved.');
      }
      throw failure;
    }
  });
  return <Dialog title="Did this go out?" close={close} busy={mutation.busy} status={error || mutation.status}
    footer={canAnswer ? <>
      <span className="ui-help outcome-foot-note">Saving lets {app} writes run again. Activity shows who answered.</span>
      <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
      <Button primary disabled={mutation.busy || happened === undefined} onClick={save}>Save answer</Button>
    </> : <Button onClick={close}>Close</Button>}>
    {!data && !error && <p role="status">Loading the action…</p>}
    {data && <div className="outcome-view">
      {waiting && <p className="outcome-callout"><PauseIcon /><span>{app} didn’t confirm this action. Cuckoo has not tried it again. {app} writes are paused
        until someone answers, so it is never done twice.</span></p>}
      <Sent data={data} app={app} />
      {waiting && <Checks settle={data.settle} app={app} />}
      {canAnswer && <fieldset className="outcome-answers">
        <legend>What happened?</legend>
        <label className={happened === true ? 'chosen' : undefined}><input type="radio" name={name} checked={happened === true} onChange={() => setHappened(true)} />
          <span><strong>It went through</strong><small>You can see it in {app}.</small></span></label>
        <label className={happened === false ? 'chosen' : undefined}><input type="radio" name={name} checked={happened === false} onChange={() => setHappened(false)} />
          <span><strong>It did not go through</strong><small>{data.agent_name} will be told, and can try again if asked.</small></span></label>
      </fieldset>}
      {canAnswer && <label className="ui-field"><span>Note <span className="ui-help">(optional)</span></span>
        <input aria-label="Note" maxLength={500} value={note} onChange={event => setNote(event.target.value)} /></label>}
      {waiting && !data.settle?.can_settle && <p className="ui-help">{data.agent_name}’s managers and workspace admins can answer this.</p>}
      {!waiting && <Answered data={data} />}
    </div>}
  </Dialog>;
}

/** The exact action and fields that were sent, who asked, who approved and which agent. */
function Sent({ data, app }: { data: Approval; app: string }) {
  return <dl className="outcome-sent">
    <div><dt>Action</dt><dd><strong>{data.title}</strong> · {app}</dd></div>
    {fieldEntries(data.arguments).map(([field, value]) => <div key={field}><dt>{field}</dt><dd>{value}</dd></div>)}
    <div><dt>Asked by</dt><dd>{[data.requester_name, data.conversation_name && `in ${data.conversation_name}`].filter(Boolean).join(' ')}{data.created ? `, ${clockTime(data.created)}` : ''}
      {data.decided_by_name && ` · approved by ${data.decided_by_name}`}</dd></div>
    <div><dt>Agent</dt><dd>{data.agent_name}</dd></div>
  </dl>;
}

function Checks({ settle, app }: { settle?: Settle | null; app: string }) {
  return <section className="outcome-checks" aria-label="What Cuckoo already checked"><h3>What Cuckoo already checked</h3>
    <ul>{(settle?.checks ?? []).map((check, index) => <li key={`${check.at}.${index}`}>{clockTime(check.at)} · {checked[check.result]?.(app) ?? check.result}</li>)}
      <li>{settle?.checks.length ? `Look in ${app} yourself, then answer below.` : `Cuckoo can’t check ${app} by itself. Look there yourself, then answer below.`}</li></ul>
  </section>;
}

function Answered({ data }: { data: Approval }) {
  const settled = data.settled;
  const happened = settled ? settled.happened : data.state === 'completed' ? true : data.state === 'not_sent' ? false : undefined;
  return <div className="outcome-answered" role="status">
    <p>{happened === undefined ? `This action is ${(states[data.state] ?? data.state).toLowerCase()}.`
      : settled ? `${settled.by_name} answered ${dayName(settled.at)} at ${clockTime(settled.at)}: ${happened ? 'it went through' : 'it did not go through'}.`
      : happened ? 'This action went through.' : 'This action did not go through.'}</p>
    {settled?.note && <p className="ui-help">Note: “{settled.note}”</p>}
  </div>;
}
