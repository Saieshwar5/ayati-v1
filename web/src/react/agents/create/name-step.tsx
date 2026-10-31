import { useEffect, useState, type RefObject } from 'react';
import { AgentFace } from '../../faces';
import type { Agent } from '../model';

/** A temporary address for the chosen file, released when it changes. */
function usePreview(file?: File) {
  const [url, setUrl] = useState<string>();
  useEffect(() => {
    if (!file || typeof URL.createObjectURL !== 'function') { setUrl(undefined); return; }
    const next = URL.createObjectURL(file);
    setUrl(next);
    return () => URL.revokeObjectURL(next);
  }, [file]);
  return url;
}

const robots = Array.from({ length: 24 }, (_, index) => `robot-${String(index + 1).padStart(2, '0')}`);

/** Create step 1: name, job title and a picture (design 1-create-name-and-picture). */
export function NameStep({ draft, file, first, change, choose, upload, reject }: {
  draft: Agent; file?: File; first: RefObject<HTMLInputElement | null>;
  change: (key: 'name' | 'role', value: string) => void;
  choose: (robot: string) => void; upload: (file: File) => void; reject: (message: string) => void;
}) {
  const preview = usePreview(file);
  return <div className="agent-create-name">
    <div className="agent-create-picture">
      {preview
        ? <img className="agent-create-face" alt="" src={preview} />
        : <AgentFace agent={{ id: draft.id, name: draft.name, avatar: draft.settings.avatar }} className="agent-create-face" />}
      <label className="ui-button agent-create-upload">Upload photo
        <input type="file" accept="image/png,image/jpeg,image/webp" onChange={event => {
          const image = event.target.files?.[0];
          event.target.value = '';
          if (!image) return;
          if (image.size > 2 * 1024 * 1024) { reject('Choose an image smaller than 2 MB.'); return; }
          upload(image);
        }} />
      </label>
      <p className="ui-help">PNG, JPEG or WebP, up to 2 MB</p>
    </div>
    <div className="ui-form agent-create-fields">
      <label className="ui-field">Name<input ref={first} required maxLength={80} value={draft.name} onChange={event => change('name', event.target.value)} /></label>
      <label className="ui-field">Job title <small>shown under the name everywhere</small>
        <input maxLength={80} placeholder="For example, Customer support" value={draft.role} onChange={event => change('role', event.target.value)} /></label>
      <fieldset className="agent-pictures"><legend>Or choose a picture</legend>
        <div className="agent-picture-grid">{robots.map(robot =>
          <button type="button" key={robot} className="agent-picture-choice" aria-label={`Robot picture ${robot.slice(6)}`}
            aria-pressed={!file && draft.settings.avatar === robot} onClick={() => choose(robot)}>
            <AgentFace agent={{ id: draft.id, name: draft.name, avatar: robot }} />
          </button>)}</div>
      </fieldset>
    </div>
  </div>;
}
