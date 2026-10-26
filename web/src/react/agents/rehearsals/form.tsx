import { useState } from 'react';
import { Button } from '../../ui';
import { workspaceUrl } from '../../../workspace-context';
import { JobSummary } from './job-summary';
import type { Options, Source } from './model';

export function SampleForm({ options, task, setTask, selected, select, reviewing, review, busy, search, refresh }: {
  options: Options; task: string; setTask: (value: string) => void; selected: Source[]; select: (value: Source[]) => void;
  reviewing: boolean; review: (value: boolean) => void; busy: boolean; search: (value: string) => void; refresh: () => void;
}) {
  const [query, setQuery] = useState('');
  if (reviewing) return <div className="sample-review"><JobSummary profile={options.profile} />
    <h3>Sample task</h3><p className="sample-task-copy">{task}</p><h3>Selected knowledge</h3>
    {selected.map(source => <p key={source.id}>{source.title} · version {source.revision}</p>)}
    {!selected.length && <p>No Library sources selected.</p>}
    <p className="ui-help">The sample uses short excerpts of these sources. Agent-wide notes are included only when enabled; private conversation notes are excluded. The saved result identifies the notes actually used.</p>
  </div>;
  return <div className="ui-form">
    <label className="ui-field">Sample task<textarea rows={4} maxLength={4000} value={task} onChange={event => setTask(event.target.value)} placeholder="Describe a real example this agent should help with." /></label>
    <section className="sample-sources" aria-label="Sample knowledge"><h3>Choose already assigned knowledge</h3>
      <p className="ui-help">Up to three prepared Library sources you can read and this agent already has access to. Selection changes no sharing or grants.</p>
      <form className="sample-search" onSubmit={event => { event.preventDefault(); search(query); }}>
        <label className="ui-field">Find assigned knowledge<input type="search" maxLength={180} value={query} onChange={event => setQuery(event.target.value)} /></label>
        <Button type="submit" disabled={busy}>Search knowledge</Button>
      </form>
      {options.sources.map(source => <label className="sample-source-option" key={source.id}>
        <input type="checkbox" checked={selected.some(value => value.id === source.id)} disabled={busy || (selected.length >= 3 && !selected.some(value => value.id === source.id))}
          onChange={event => select(event.target.checked ? [...selected, source] : selected.filter(value => value.id !== source.id))} />
        <span>{source.title}<small>Version {source.revision} · {source.visibility === 'private' ? 'Private source' : 'Shared source'}{source.readiness === 'partial' ? ' · Partial text' : ''}</small></span>
      </label>)}
      {!options.sources.length && <p>No matching assigned sources. A source steward can assign this agent from <a href={workspaceUrl('/?view=library')}>Library</a>.</p>}
      {options.more && <p className="ui-help">More sources are available. Narrow your search.</p>}
      {selected.length > 0 && <p className="ui-help">Selected: {selected.map(source => `${source.title} (v${source.revision})`).join(', ')}</p>}
    </section>
    <p className="ui-help">Uses the configured model; normal model usage may apply. Starting is explicit. Opening this view or refreshing status does not run a model.</p>
    <Button disabled={busy} onClick={refresh}>Refresh saved job and knowledge</Button>
    <Button primary disabled={busy || !task.trim() || Boolean(options.active_sample)} onClick={() => review(true)}>Review sample context</Button>
    {options.active_sample && <p>A sample is already active. Open or stop it before starting another for this agent.</p>}
  </div>;
}
