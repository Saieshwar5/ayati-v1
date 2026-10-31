import { useState } from 'react';
import { Button, Dialog } from '../../ui';
import { useResource } from '../../requests';
import { loadLibrary, scope, type Skill } from './model';
import { SkillUpload } from './skill-upload';
import './skills.css';

/** Add skills to an agent: pick from the workspace's skills and your own, or
 * upload a new one (D234). `owner` owns the agent; personal skills only fit
 * agents their owner owns. */
export function SkillPicker({ owner, chosen, choose, close }: {
  owner: string; chosen: string[]; choose: (skill: Skill) => Promise<void> | void; close: () => void;
}) {
  const library = useResource(loadLibrary);
  const [uploading, setUploading] = useState(false);
  const [status, setStatus] = useState('');
  const [busy, setBusy] = useState<string>();
  const data = library.data;
  const usable = (skill: Skill) => skill.shared || skill.owner_id === owner;
  const add = async (skill: Skill) => {
    if (busy) return;
    setBusy(skill.id); setStatus('');
    try { await choose(skill); setStatus(`${skill.name} added.`); }
    catch (failure) { setStatus(failure instanceof Error ? failure.message : 'Could not add the skill.'); }
    finally { setBusy(undefined); }
  };
  const row = (skill: Skill) => <li key={skill.id}>
    <span><strong>{skill.name}</strong><small>{scope(skill)} · version {skill.version}{skill.has_scripts ? ' · scripts can’t run yet' : ''}</small>
      <small>{skill.description}</small></span>
    {chosen.includes(skill.id) ? <span className="ui-badge">Added</span>
      : <Button disabled={!usable(skill) || Boolean(busy)} title={usable(skill) ? undefined : 'Only for agents its owner owns'} onClick={() => void add(skill)}>Add</Button>}
  </li>;
  return <Dialog drawer title="Add skills" close={close} status={status}>
    <p className="ui-help">Skills are how-to steps an agent loads only when a request needs them.</p>
    {library.error && <p role="alert">{library.error}</p>}
    {!data && !library.error && <p role="status">Loading skills…</p>}
    {data && <>
      <h3 className="skill-group">Workspace skills</h3>
      {data.workspace.length ? <ul className="skill-list">{data.workspace.map(row)}</ul> : <p className="ui-help">No workspace skills yet.</p>}
      <h3 className="skill-group">My skills</h3>
      {data.mine.length ? <ul className="skill-list">{data.mine.map(row)}</ul> : <p className="ui-help">You haven’t added any skills.</p>}
      {data.admin && data.others.length > 0 && <><h3 className="skill-group">Other people’s skills</h3><ul className="skill-list">{data.others.map(row)}</ul></>}
      {uploading
        ? <SkillUpload admin={data.admin} saved={skill => { setUploading(false); void library.refresh().catch(() => {}); void add({ ...skill, files: skill.files.length, agents: [] }); }} />
        : <Button onClick={() => setUploading(true)}>Upload a new skill</Button>}
    </>}
  </Dialog>;
}
