import { useCallback, useState, type ReactNode } from 'react';
import { Button, Dialog } from '../../ui';
import { useResource } from '../../requests';
import { download, loadSkill, scope } from './model';
import './skills.css';

/** A skill's files, versions and a standard `.zip` download. */
export function SkillView({ id, close, footer }: { id: string; close: () => void; /** Such as Remove, on an agent's page. */ footer?: ReactNode }) {
  const [version, setVersion] = useState<number>();
  const load = useCallback(() => loadSkill(id, version), [id, version]);
  const { data, error } = useResource(load);
  const main = data?.files.find(file => file.path === 'SKILL.md');
  return <Dialog drawer title={data ? data.name : 'Skill'} close={close} footer={footer}>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading skill…</p>}
    {data && <section className="skill-detail" aria-label="Skill">
      <p><strong>Use it when:</strong> {data.description}</p>
      <p className="ui-help">{scope(data)} · {data.license ? `License: ${data.license}` : 'No license'}{data.has_scripts ? ' · Has scripts: they run on the agent’s computer under its programs rule' : ''}</p>
      <label className="ui-field">Version<select value={data.shown_version} onChange={event => setVersion(Number(event.target.value))}>
        {data.versions.map(v => <option key={v.version} value={v.version}>Version {v.version} · {v.author} · {new Date(v.created_ms).toLocaleDateString()}</option>)}</select></label>
      {main && <pre className="skill-text">{main.text}</pre>}
      <ul className="skill-files">{data.files.filter(file => file.path !== 'SKILL.md').map(file => <li key={file.path}>
        <details><summary>{file.path}</summary><pre>{file.text}</pre></details></li>)}</ul>
      <Button onClick={() => download(data.name, data.files)}>Download .zip</Button>
    </section>}
  </Dialog>;
}
