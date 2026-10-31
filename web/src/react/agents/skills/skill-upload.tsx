import { useRef, useState } from 'react';
import { Button } from '../../ui';
import { errorMessage, useMutation } from '../../requests';
import { checkSkill, createSkill, readFolder, readZip, type Checked, type SkillDetail, type SkillFile } from './model';

const size = (text: string) => { const n = new TextEncoder().encode(text).length; return n < 1024 ? `${n} B` : `${(n / 1024).toFixed(1)} KB`; };

/** Upload a skill: a `.zip`, a folder or a pasted SKILL.md. Cuckoo checks it and
 * shows every file before anything is saved (D234). */
export function SkillUpload({ admin, saved }: { admin: boolean; saved: (skill: SkillDetail) => void }) {
  const [files, setFiles] = useState<SkillFile[]>();
  const [checked, setChecked] = useState<Checked>();
  const [problem, setProblem] = useState('');
  const [pasted, setPasted] = useState('');
  const [shared, setShared] = useState(admin);
  const id = useRef(crypto.randomUUID());
  const reading = useMutation();
  const saving = useMutation();
  const review = (read: () => Promise<SkillFile[]> | SkillFile[]) => void reading.run(async () => {
    setProblem(''); setChecked(undefined); setFiles(undefined);
    try {
      const next = await read();
      setFiles(next);
      setChecked(await checkSkill(next));
      id.current = crypto.randomUUID();
    } catch (failure) { setProblem(errorMessage(failure)); }
  });
  const save = () => void saving.run(async () => {
    if (!files) return;
    saved(await createSkill(id.current, shared, files));
  });
  return <div className="skill-upload">
    <div className="skill-upload-sources">
      <label className="ui-button">Upload a .zip<input type="file" accept=".zip,application/zip" hidden
        onChange={event => { const file = event.target.files?.[0]; event.target.value = ''; if (file) review(async () => readZip(new Uint8Array(await file.arrayBuffer()))); }} /></label>
      <label className="ui-button">Upload a folder<input type="file" hidden {...{ webkitdirectory: '' }}
        onChange={event => { const list = event.target.files; if (list?.length) review(() => readFolder(list)); event.target.value = ''; }} /></label>
    </div>
    <label className="ui-field">Or paste a SKILL.md<textarea rows={6} value={pasted} onChange={event => setPasted(event.target.value)}
      placeholder={'---\nname: quote-builder\ndescription: Use when a customer asks for a quote\n---\n1. …'} /></label>
    <Button disabled={!pasted.trim() || reading.busy} onClick={() => review(() => [{ path: 'SKILL.md', text: pasted }])}>Check pasted skill</Button>
    {reading.busy && <p role="status">Checking the skill…</p>}
    {problem && <p role="alert">{problem}</p>}
    {checked && files && <section className="skill-review" aria-label="Review">
      <h4>{checked.name}</h4>
      <p><strong>Use it when:</strong> {checked.description}</p>
      <p className="ui-help">{checked.license ? `License: ${checked.license}` : 'No license'}{checked.has_scripts ? ' · Has scripts: scripts can’t run yet' : ''}</p>
      <ul className="skill-files">{files.map(file => <li key={file.path}><details><summary>{file.path} <small>{size(file.text)}</small></summary>
        <pre>{file.text}</pre></details></li>)}</ul>
      {admin && <label className="ui-check"><input type="checkbox" checked={shared} onChange={event => setShared(event.target.checked)} />Share with the workspace (any agent can use it)</label>}
      <Button primary disabled={saving.busy} onClick={save}>{saving.busy ? 'Saving…' : 'Save skill'}</Button>
      {saving.status && <p role="alert">{saving.status}</p>}
    </section>}
  </div>;
}
