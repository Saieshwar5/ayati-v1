import { Button } from '../../ui';
import { ResponsePreview } from '../../results/preview';
import { workspaceUrl } from '../../../workspace-context';
import { JobSummary } from './job-summary';
import { activeSample, sampleLabel, type Sample } from './model';
export function SampleDetails({ data, error, loading, busy, canStop, currentRevision, stop, refresh, read }: {
  data?: Sample; error: string; loading: boolean; busy: boolean; canStop: boolean; currentRevision?: number;
  stop: () => void; refresh: () => void; read: () => void;
}) {
  return <section aria-label="Saved sample">
    {error && <p role="alert">{error}</p>}{!data && !error && <p role="status">Checking saved sample…</p>}
    {data && <><p className="sample-state" role="status">{sampleLabel(data.state)}</p><p>{data.detail}</p>
      {activeSample(data.state) && <p className="ui-help">You can leave this view. The saved sample continues, and status can be checked without another model request.</p>}
      {currentRevision !== undefined && currentRevision !== data.profile.revision && <p role="status">The job changed since this sample. It uses the saved job version shown below.</p>}
      <h3>Sample task</h3><p className="sample-task-copy">{data.input.task}</p>
      {data.state === 'completed' && data.preview && <section className="sample-output" aria-label="Draft-only sample response">
        <h3>Draft-only response</h3><ResponsePreview id={data.id} text={data.preview} reference={data.response} open={read} />
        {!data.response?.long && <Button onClick={read}>Open saved response</Button>}
      </section>}
      <JobSummary profile={data.profile} />
      <section><h3>Knowledge used</h3>{data.sources.map(source => <p key={source.id}><a href={workspaceUrl(`/?view=library&item=${encodeURIComponent(source.id)}`)}>{source.title}</a> · version {source.revision}</p>)}
        {data.note_refs.map(note => <p key={note.id}>{note.title} · approved note version {note.revision}</p>)}
        {!data.sources.length && !data.note_refs.length && <p>No Library sources or approved notes were included.</p>}
        <p className="ui-help">Current access and source versions are checked again when opening a sample. Corrected or withdrawn knowledge can make an older sample unavailable.</p>
      </section>
    </>}
    <div className="sample-actions"><Button disabled={loading || busy} onClick={refresh}>Refresh sample status</Button>
      {(canStop || activeSample(data?.state)) && <Button disabled={busy} onClick={stop}>Stop sample</Button>}
    </div>
    <p className="ui-help">A sample is a draft response, not proof of every future behavior or of a business operation. Permission previews and actual work retain their separate controls.</p>
  </section>;
}
