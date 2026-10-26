import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { ResponseReader } from '../../results/reader';
import type { Agent } from '../model';
import { SampleForm } from './form';
import { SampleDetails } from './details';
import { SampleHistory } from './history';
import { activeSample, loadHistory, loadOptions, sampleSource, startSample, stopSample, type SampleInput, type Source } from './model';
import { useSample } from './use-sample';
import './rehearsals.css';
import '../../results/results.css';
import { subscribeChanges } from '../../chat/live';

export function RehearsalDialog({ agent, close }: { agent: Agent; close: () => void }) {
  return <SampleFlow key={agent.id} agent={agent} close={close} />;
}
function SampleFlow({ agent, close }: { agent: Agent; close: () => void }) {
  const [search, setSearch] = useState('');
  const optionsLoad = useCallback(() => loadOptions(agent.id, search), [agent.id, search]);
  const options = useResource(optionsLoad);
  const [cursors, setCursors] = useState<(string | null)[]>([null]);
  const before = cursors.at(-1) ?? null;
  const historyLoad = useCallback(() => loadHistory(agent.id, before), [agent.id, before]);
  const history = useResource(historyLoad);
  const [screen, setScreen] = useState<'new' | 'history' | 'sample'>('new');
  const [selected, select] = useState('');
  const [task, setTask] = useState('');
  const [sources, setSources] = useState<Source[]>([]);
  const [reviewing, review] = useState(false);
  const [reading, read] = useState(false);
  const current = useSample(agent.id, screen === 'sample' ? selected : '');
  const mutation = useMutation();
  const alive = useRef(false);
  const attempt = useRef<{ payload: string; id: string } | undefined>(undefined);
  const resultSource = useMemo(() => sampleSource(agent.id), [agent.id]);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => subscribeChanges(change => {
    if (change.kind !== 'sample_access') return;
    options.setData(undefined); history.setData(undefined); setSources([]); read(false); review(false);
    void Promise.allSettled([options.refresh(), history.refresh()]);
  }), [options.setData, history.setData, options.refresh, history.refresh]);
  const refreshLists = async () => { await Promise.allSettled([options.refresh(), history.refresh()]); };
  const fresh = () => { setTask(''); setSources([]); review(false); attempt.current = undefined; mutation.setStatus(''); setScreen('new'); select(''); void refreshLists(); };
  const open = (id: string) => { select(id); setScreen('sample'); mutation.setStatus(''); };
  const start = () => void mutation.run(async () => {
    if (!options.data?.execution_available) throw new Error('The configured model is unavailable here.');
    const input = { task, agent_revision: options.data.profile.revision, sources: sources.map(({ id, revision, access_revision }) => ({ id, revision, access_revision })) };
    const payload = JSON.stringify(input);
    if (attempt.current?.payload !== payload) attempt.current = { payload, id: crypto.randomUUID() };
    const saved = await startSample(agent.id, { ...input, id: attempt.current.id } satisfies SampleInput);
    if (alive.current) { open(saved.id); await refreshLists(); }
  });
  const stop = () => void mutation.run(async () => {
    await stopSample(agent.id, selected);
    if (alive.current) { await current.refresh(); await refreshLists(); }
  });
  const showHistory = () => { setScreen('history'); void history.refresh().catch(() => {}); };
  const footer = screen === 'new' && reviewing && <>
    <Button disabled={mutation.busy} onClick={() => review(false)}>Back to sample</Button>
    <Button primary disabled={mutation.busy || !options.data?.execution_available || Boolean(options.data.active_sample)} onClick={start}>Start draft-only sample</Button>
  </>;
  return <Dialog drawer title={`${agent.name} · Try a sample task`} close={close} busy={mutation.busy} status={mutation.status} footer={footer}>
    <div className="agent-rehearsal"><p className="ui-help">Draft-only rehearsal. No connected actions, browser or computer. Saved samples are visible only to you.</p>
      <nav className="sample-actions" aria-label="Sample views"><Button aria-pressed={screen === 'new'} onClick={fresh}>New sample</Button><Button aria-pressed={screen === 'history'} onClick={showHistory}>Saved samples</Button></nav>
      {options.data?.active_sample && selected !== options.data.active_sample.id && <Button onClick={() => open(options.data!.active_sample!.id)}>Open active sample</Button>}
      {options.error && <p role="alert">{options.error}<Button onClick={() => void options.refresh().catch(() => {})}>Retry job and knowledge</Button></p>}
      {screen === 'new' && <>
        {!options.data && !options.error && <p role="status">Loading saved job and eligible knowledge…</p>}
        {options.data?.execution_available === false && <p role="status">The model is not configured here. Ask your operator to enable sample execution.</p>}
        {options.data && <SampleForm options={options.data} task={task} setTask={setTask} selected={sources} select={setSources} reviewing={reviewing} review={review} busy={mutation.busy}
          search={value => { setSearch(value); if (value === search) void options.refresh().catch(() => {}); }} refresh={() => void options.refresh().catch(() => {})} />}
      </>}
      {screen === 'history' && <>{history.error && <p role="alert">{history.error}</p>}<SampleHistory data={history.data} loading={!history.data && !history.error}
        open={open} older={cursor => setCursors(old => [...old, cursor])} newer={cursors.length > 1 ? () => setCursors(old => old.slice(0, -1)) : undefined}
        refresh={() => { if (cursors.length > 1) setCursors([null]); else void history.refresh().catch(() => {}); }} /></>}
      {screen === 'sample' && <SampleDetails data={current.data} error={current.error} loading={current.loading} busy={mutation.busy}
        canStop={options.data?.active_sample?.id === selected} currentRevision={options.data?.profile.revision ?? agent.revision}
        stop={stop} refresh={() => { void current.refresh(); void refreshLists(); }} read={() => read(true)} />}
      {screen === 'sample' && current.data && !activeSample(current.data.state) && <Button onClick={fresh}>Try another sample</Button>}
      {reading && selected && <ResponseReader id={selected} source={resultSource} close={() => read(false)} />}
    </div>
  </Dialog>;
}
