import { useId, useRef, useState, type DragEvent } from 'react';
import { Button } from '../../ui';
import { errorMessage, useMutation } from '../../requests';
import { byteSize, checkSkill, createSkill, isScript, readDropped, readFolder, readZip, textBytes,
  type Checked, type SkillDetail, type SkillFile } from './model';
import { SkillIcon } from './skill-icons';

type Read = { source: string; files: SkillFile[] };
type Reviewed = Read & { checked: Checked };
export type SkillDraft = ReturnType<typeof useSkillDraft>;

/** The upload step's draft (design 8). The picker owns it so the panel footer can
 * save it. `reset` discards it, and any check still on its way. Nothing is saved
 * until `save`, which creates the skill once and retries only what failed. */
export function useSkillDraft() {
  const [reviewed, setReviewed] = useState<Reviewed>();
  const [problem, setProblem] = useState('');
  const [checking, setChecking] = useState(false);
  const [shared, setShared] = useState(false);
  const [saved, setSaved] = useState<SkillDetail>();
  const generation = useRef(0);
  const id = useRef('');
  const saving = useMutation();
  const clear = () => {
    generation.current++;
    setReviewed(undefined); setProblem(''); setChecking(false); setSaved(undefined); saving.setStatus('');
  };
  const review = async (read: () => Promise<Read>) => {
    clear();
    const current = generation.current;
    setChecking(true);
    try {
      const next = await read();
      const checked = await checkSkill(next.files);
      if (generation.current !== current) return;
      // A new key for each checked payload, so a retry of the same files is one skill.
      id.current = crypto.randomUUID();
      setReviewed({ ...next, checked });
    } catch (failure) {
      if (generation.current === current) setProblem(errorMessage(failure));
    } finally {
      if (generation.current === current) setChecking(false);
    }
  };
  return {
    reviewed, problem, checking, shared, saved, saving: saving.busy, status: saving.status, review, clear,
    reset: (share = false) => { clear(); setShared(share); },
    share: (value: boolean) => { setShared(value); id.current = crypto.randomUUID(); },
    /** Create the skill (once), then hand it to `attach`; failures keep the step. */
    save: (attach: (skill: SkillDetail) => Promise<void>) => saving.run(async () => {
      if (!reviewed) return;
      const skill = saved ?? await createSkill(id.current, shared, reviewed.files);
      setSaved(skill);
      await attach(skill);
    }),
  };
}

/** Upload a skill: choose or drop a `.zip` or folder, or paste a SKILL.md; then
 * review what Cuckoo read and every file before Add skill (D234). */
export function SkillUpload({ draft, agent, admin, shareNeeded }: {
  draft: SkillDraft; agent: string; admin: boolean; shareNeeded: boolean;
}) {
  const zip = useRef<HTMLInputElement>(null);
  const folder = useRef<HTMLInputElement>(null);
  const [pasting, setPasting] = useState(false);
  const [pasted, setPasted] = useState('');
  const [dragging, setDragging] = useState(false);
  const drop = (event: DragEvent) => {
    event.preventDefault();
    setDragging(false);
    // Dropped items are readable only during the event.
    const entries = [...event.dataTransfer.items].map(item => item.webkitGetAsEntry?.() ?? null);
    void draft.review(() => readDropped(entries, [...event.dataTransfer.files]));
  };
  if (draft.reviewed) return <Review draft={draft} reviewed={draft.reviewed} agent={agent} admin={admin} shareNeeded={shareNeeded} />;
  return <div className="skill-upload">
    <div className={`skill-drop${dragging ? ' dragging' : ''}`} onDrop={drop}
      onDragOver={event => { event.preventDefault(); setDragging(true); }} onDragLeave={() => setDragging(false)}>
      <span className="skill-drop-icon"><SkillIcon name="upload" /></span>
      <strong>Drop a .zip or a skill folder here</strong>
      <p className="ui-help">Cuckoo checks it before anything is saved.</p>
      <div className="skill-drop-choices">
        <Button autoFocus onClick={() => zip.current?.click()}>Choose .zip</Button>
        <Button onClick={() => folder.current?.click()}>Choose folder</Button>
      </div>
      {!pasting && <Button className="skill-link" onClick={() => setPasting(true)}>or paste a SKILL.md</Button>}
      <input ref={zip} type="file" accept=".zip,application/zip" hidden onChange={event => {
        const file = event.target.files?.[0];
        event.target.value = '';
        if (file) void draft.review(async () => ({ source: file.name, files: readZip(new Uint8Array(await file.arrayBuffer())) }));
      }} />
      <input ref={folder} type="file" hidden {...{ webkitdirectory: '' }} onChange={event => {
        const list = [...event.target.files ?? []];
        event.target.value = '';
        if (list.length) void draft.review(async () => ({ source: list[0].webkitRelativePath.split('/')[0] || 'Folder', files: await readFolder(list) }));
      }} />
    </div>
    {pasting && <div className="skill-paste">
      <label className="ui-field">Paste a SKILL.md
        <textarea autoFocus rows={8} value={pasted} onChange={event => setPasted(event.target.value)}
          placeholder={'---\nname: quote-builder\ndescription: Use when a customer asks for a quote\n---\n1. …'} />
      </label>
      <Button disabled={!pasted.trim()} onClick={() => void draft.review(async () => ({ source: 'Pasted SKILL.md', files: [{ path: 'SKILL.md', text: pasted }] }))}>
        Check SKILL.md</Button>
    </div>}
    {draft.checking && <p className="ui-help" role="status">Checking the skill…</p>}
    {draft.problem && <p className="skill-problem" role="alert">{draft.problem}</p>}
  </div>;
}

function Review({ draft, reviewed, agent, admin, shareNeeded }: {
  draft: SkillDraft; reviewed: Reviewed; agent: string; admin: boolean; shareNeeded: boolean;
}) {
  const { source, files, checked } = reviewed;
  const heading = useId();
  const total = files.reduce((sum, file) => sum + textBytes(file.text), 0);
  const ordered = [...files].sort((a, b) => Number(b.path === 'SKILL.md') - Number(a.path === 'SKILL.md'));
  return <div className="skill-upload">
    <div className="skill-source">
      <span className="skill-tile"><SkillIcon name="folder" /></span>
      <span className="skill-source-text"><strong>{source}</strong>
        <small>{files.length} {files.length === 1 ? 'file' : 'files'} · {byteSize(total)} · checked</small></span>
      <Button className="skill-link" autoFocus onClick={draft.clear}>Choose another</Button>
    </div>
    <dl className="skill-facts">
      <div><dt>Name</dt><dd>{checked.name}</dd></div>
      <div><dt>License</dt><dd>{checked.license ?? 'None given'}</dd></div>
      <div className="skill-facts-wide"><dt>Use it when</dt><dd>{checked.description}</dd></div>
    </dl>
    <section className="skill-file-section" aria-labelledby={heading}>
      <h3 id={heading}>Files</h3>
      <ul className="skill-file-list">{ordered.map(file => <FileRow key={file.path} file={file} initiallyOpen={file.path === 'SKILL.md'} />)}</ul>
    </section>
    {checked.has_scripts && <p className="skill-warning"><SkillIcon name="warning" />
      <span>This skill has a script. Scripts can’t run yet, so {agent} follows the written steps only.</span></p>}
    {admin && <label className="ui-check skill-share">
      <input type="checkbox" checked={draft.shared} disabled={Boolean(draft.saved)} onChange={event => draft.share(event.target.checked)} />
      <span>Share with the workspace<small>Any agent can use it. Admins only.</small></span>
    </label>}
    {admin && shareNeeded && !draft.shared && <p className="skill-warning"><SkillIcon name="warning" />
      <span>A skill you keep to yourself only works on your own agents, so {agent} can’t use it.</span></p>}
  </div>;
}

const PREVIEW_LINES = 8;

/** One file: opening it previews it in place, with Show all for long files. No
 * nested scroll box: the panel is the only scroll area. */
function FileRow({ file, initiallyOpen }: { file: SkillFile; initiallyOpen: boolean }) {
  const [open, setOpen] = useState(initiallyOpen);
  const [all, setAll] = useState(false);
  const preview = useId();
  const lines = file.text.replace(/\n$/, '').split('\n');
  return <li>
    <button type="button" className="skill-file-toggle" aria-expanded={open} aria-controls={open ? preview : undefined} onClick={() => setOpen(!open)}>
      <SkillIcon name="file" /><span className="skill-file-path">{file.path}</span>
      <small>{byteSize(textBytes(file.text))}</small>
      {isScript(file.path) && <span className="skill-file-tag">Can’t run yet</span>}
      <SkillIcon name="chevron" />
    </button>
    {open && <div className="skill-file-preview" id={preview}>
      <pre>{all ? file.text : lines.slice(0, PREVIEW_LINES).join('\n')}</pre>
      {lines.length > PREVIEW_LINES && <Button className="skill-link" onClick={() => setAll(!all)}>
        {all ? 'Show less' : `Show all ${lines.length} lines`}</Button>}
    </div>}
  </li>;
}
