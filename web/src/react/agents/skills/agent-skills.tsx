import { useCallback, useEffect, useRef, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { useMutation, useResource } from '../../requests';
import type { Agent } from '../model';
import { assignSkill, download, loadAgentSkills, loadSkill, scope, type Skill } from './model';
import { SkillPicker } from './skill-picker';
import './skills.css';

/** "Skills it can use" on the Instructions tab (designs/agents 8, D234). */
export function AgentSkills({ agent }: { agent: Agent }) {
  const load = useCallback(() => loadAgentSkills(agent.id), [agent.id]);
  const { data, error, setData } = useResource(load);
  const [adding, setAdding] = useState(false);
  const [viewing, setViewing] = useState<Skill>();
  const mutation = useMutation();
  const remove = (skill: Skill) => void mutation.run(async () => { setData(await assignSkill(agent.id, skill.id, false)); mutation.setStatus(`${skill.name} removed.`); });
  const editable = data?.editable ?? false;
  const skills = data?.skills ?? [];
  return <section className="agent-skills" aria-label="Skills it can use">
    <header className="agent-section-heading"><h3>Skills it can use</h3>
      {editable && <Button onClick={() => setAdding(true)}>＋ Add skill</Button>}</header>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading skills…</p>}
    {data && !skills.length && <p className="ui-help">No skills yet. Skills are how-to steps {agent.name} loads only when a request needs them.</p>}
    <ul className="skill-list">{skills.map(skill => <li key={skill.id}>
      <span><strong>{skill.name}</strong><small>{scope(skill)} · version {skill.version}{skill.used_this_week ? ` · used ${skill.used_this_week} ${skill.used_this_week === 1 ? 'time' : 'times'} this week` : ''}</small></span>
      <SkillMenu name={skill.name} editable={editable} view={() => setViewing(skill)} remove={() => remove(skill)} />
    </li>)}</ul>
    <p className="ui-status" role="status">{mutation.status}</p>
    {adding && <SkillPicker owner={agent.owner_id ?? ''} chosen={skills.map(s => s.id)} close={() => setAdding(false)}
      choose={async skill => { setData(await assignSkill(agent.id, skill.id, true)); }} />}
    {viewing && <SkillView id={viewing.id} close={() => setViewing(undefined)} />}
  </section>;
}

function SkillMenu({ name, editable, view, remove }: { name: string; editable: boolean; view: () => void; remove: () => void }) {
  const [open, setOpen] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => { if (open) menu.current?.querySelector<HTMLButtonElement>('button')?.focus(); }, [open]);
  const pick = (action: () => void) => { setOpen(false); toggle.current?.focus(); action(); };
  return <span className="skill-menu" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false); }}>
    <Button ref={toggle} className="ui-icon" aria-label={`Actions for ${name}`} aria-expanded={open} aria-haspopup="menu" onClick={() => setOpen(!open)}>⋯</Button>
    {open && <div ref={menu} className="chat-message-menu" role="menu" aria-label={`Actions for ${name}`}
      onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); setOpen(false); toggle.current?.focus(); } }}>
      <button type="button" role="menuitem" onClick={() => pick(view)}>View and download</button>
      {editable && <button type="button" role="menuitem" className="danger" onClick={() => pick(remove)}>Remove from this agent</button>}
    </div>}
  </span>;
}

/** A skill's files, versions and a standard `.zip` download. */
export function SkillView({ id, close }: { id: string; close: () => void }) {
  const [version, setVersion] = useState<number>();
  const load = useCallback(() => loadSkill(id, version), [id, version]);
  const { data, error } = useResource(load);
  const main = data?.files.find(file => file.path === 'SKILL.md');
  return <Dialog drawer title={data ? data.name : 'Skill'} close={close}>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading skill…</p>}
    {data && <section className="skill-detail" aria-label="Skill">
      <p><strong>Use it when:</strong> {data.description}</p>
      <p className="ui-help">{scope(data)} · {data.license ? `License: ${data.license}` : 'No license'}{data.has_scripts ? ' · Has scripts: scripts can’t run yet' : ''}</p>
      <label className="ui-field">Version<select value={data.shown_version} onChange={event => setVersion(Number(event.target.value))}>
        {data.versions.map(v => <option key={v.version} value={v.version}>Version {v.version} · {v.author} · {new Date(v.created_ms).toLocaleDateString()}</option>)}</select></label>
      {main && <pre className="skill-text">{main.text}</pre>}
      <ul className="skill-files">{data.files.filter(file => file.path !== 'SKILL.md').map(file => <li key={file.path}>
        <details><summary>{file.path}</summary><pre>{file.text}</pre></details></li>)}</ul>
      <Button onClick={() => download(data.name, data.files)}>Download .zip</Button>
    </section>}
  </Dialog>;
}
