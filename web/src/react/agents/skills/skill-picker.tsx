import { useRef, useState, type ReactNode } from 'react';
import { flushSync } from 'react-dom';
import { Button, Dialog } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { workspaceSession } from '../../../workspace-context';
import { loadLibrary, type Skill } from './model';
import { SkillIcon } from './skill-icons';
import { SkillUpload, useSkillDraft } from './skill-upload';
import './skills.css';

/** Add skills to an agent (designs/chat-and-account 7–8, D245): one panel with a
 * search, short groups and flat rows where Add attaches at once, and Upload a skill
 * as a second step in the same panel. `owner` owns the agent; personal skills only
 * fit agents their owner owns (D234). */
export function SkillPicker({ agent, owner, chosen, choose, close }: {
  agent: string; owner: string; chosen: string[]; choose: (skill: Skill) => Promise<void> | void; close: () => void;
}) {
  const me = workspaceSession?.user?.id ?? '';
  const library = useResource(loadLibrary);
  const [uploading, setUploading] = useState(false);
  const [query, setQuery] = useState('');
  const [pending, setPending] = useState<string>();
  const adding = useMutation();
  const draft = useSkillDraft();
  const search = useRef<HTMLInputElement>(null);
  const uploadButton = useRef<HTMLButtonElement>(null);
  const name = agent.trim() || 'this agent';
  const data = library.data;
  const admin = data?.admin ?? false;
  const add = (skill: Skill) => void adding.run(async () => {
    setPending(skill.id);
    try { await choose(skill); } finally { setPending(undefined); }
  });
  const openUpload = () => { adding.setStatus(''); draft.reset(admin && owner !== me); setUploading(true); };
  // Back, Cancel and a finished upload return to the list, with focus on Upload a skill.
  const showList = () => { draft.reset(); flushSync(() => setUploading(false)); uploadButton.current?.focus(); };
  const addUploaded = () => void draft.save(async skill => {
    void library.refresh().catch(() => {});
    await choose({ ...skill, files: skill.files.length, agents: [] });
    showList();
  });
  // Failures stay in the footer, next to the action, however far the panel is scrolled.
  const failure = uploading ? draft.status : adding.status;
  const footer = <>
    {failure && <p className="skill-panel-error" role="alert">{failure}</p>}
    {uploading ? <>
      <Button disabled={draft.saving} onClick={showList}>Cancel</Button>
      <Button primary disabled={!draft.reviewed || draft.saving} onClick={addUploaded}>{draft.saving ? 'Adding…' : 'Add skill'}</Button>
    </> : <>
      <p className="skill-picker-count" role="status">{chosen.length} added to {name}</p>
      <Button ref={uploadButton} disabled={!data} onClick={openUpload}><SkillIcon name="upload" />Upload a skill</Button>
      <Button primary onClick={close}>Done</Button>
    </>}
  </>;
  return <Dialog drawer title={uploading ? 'Upload a skill' : 'Add skills'} back={uploading ? showList : undefined} close={close}
    busy={draft.saving} footer={footer} initialFocus={search}>
    {uploading
      ? <SkillUpload draft={draft} agent={name} admin={admin} shareNeeded={owner !== me} />
      : <div className="skill-picker">
        <p className="ui-help">How-to steps {name} loads only when a request needs them.</p>
        <label className="skill-search"><SkillIcon name="search" />
          <input ref={search} type="search" className="ui-search" aria-label="Search skills" placeholder="Search skills"
            value={query} onChange={event => setQuery(event.target.value)} /></label>
        {library.error && <p role="alert">{library.error}</p>}
        {!data && !library.error && <p className="ui-help" role="status">Loading skills…</p>}
        {data && <SkillGroups query={query} groups={[
          { title: 'In this workspace', skills: data.workspace, empty: 'No workspace skills yet.' },
          { title: 'Yours', skills: data.mine, empty: 'Skills you upload show here.' },
          ...(data.admin && data.others.length ? [{ title: 'Other people’s', skills: data.others, empty: '' }] : []),
        ]} row={skill => <SkillRow key={skill.id} skill={skill} mine={skill.owner_id === me}
          usable={skill.shared || skill.owner_id === owner} added={chosen.includes(skill.id)}
          pending={pending === skill.id} busy={Boolean(pending)} add={() => add(skill)} />} />}
      </div>}
  </Dialog>;
}

type Group = { title: string; skills: Skill[]; empty: string };

/** Groups filtered by one search across name, description and owner. */
function SkillGroups({ query, groups, row }: { query: string; groups: Group[]; row: (skill: Skill) => ReactNode }) {
  const words = query.trim().toLowerCase();
  const matches = (skill: Skill) => !words || [skill.name, skill.description, skill.owner_name].some(text => text.toLowerCase().includes(words));
  const shown = groups.map(group => ({ ...group, skills: group.skills.filter(matches) })).filter(group => !words || group.skills.length);
  if (!shown.length) return <p className="ui-help">No skills match “{query.trim()}”.</p>;
  return shown.map(group => <section key={group.title} className="skill-group-section">
    <h3 className="skill-group">{group.title} · {group.skills.length}</h3>
    {group.skills.length ? <ul className="skill-rows">{group.skills.map(row)}</ul> : <p className="ui-help">{group.empty}</p>}
  </section>);
}

const possessive = (name: string) => { const first = name.trim().split(/\s+/)[0]; return first ? `${first}’s` : 'its owner’s'; };

/** A flat row: book icon, name, one-line description, owner · version, and Add,
 * Added or the reason it can't be used (as text, so touch screens show it too).
 * The action stays one button while it changes, so keyboard focus isn't lost. */
function SkillRow({ skill, mine, usable, added, pending, busy, add }: {
  skill: Skill; mine: boolean; usable: boolean; added: boolean; pending: boolean; busy: boolean; add: () => void;
}) {
  const meta = skill.shared ? `${skill.owner_name} · version ${skill.version}`
    : mine ? `Only you · version ${skill.version}` : `${skill.owner_name} · personal`;
  return <li className={`skill-row${usable || added ? '' : ' unusable'}`}>
    <span className="skill-tile"><SkillIcon name="book" /></span>
    <span className="skill-row-text"><strong>{skill.name}</strong><span>{skill.description}</span><small>{meta}</small></span>
    {added || usable
      ? <Button className={added ? 'skill-added' : undefined} aria-label={added ? `${skill.name} added` : `Add ${skill.name}`}
        aria-disabled={added || busy || undefined} tabIndex={added ? -1 : undefined} onClick={() => { if (!added) add(); }}>
        {added ? <><SkillIcon name="check" />Added</> : pending ? 'Adding…' : 'Add'}</Button>
      : <span className="skill-reason">{mine ? 'Only for your agents' : `Only for ${possessive(skill.owner_name)} agents`}</span>}
  </li>;
}
