import { useEffect, useRef, useState } from 'react';
import { identityRequest, workspaceId, workspaceSession } from '../../workspace-context';
import { PersonFace } from '../faces';
import { errorMessage, useMutation } from '../requests';
import { Button } from '../ui';

type MemberProfile = { id: string; name: string; title: string; about: string; photo: string | null };
const PHOTO_LIMIT = 2 * 1024 * 1024;

/** What colleagues see on your profile card and next to your messages. */
export function MyProfile() {
  const user = workspaceSession?.user;
  const [saved, setSaved] = useState<MemberProfile>();
  const [draft, setDraft] = useState({ name: '', title: '', about: '' });
  const [loadError, setLoadError] = useState('');
  const mutation = useMutation();
  const photo = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const abort = new AbortController();
    identityRequest<{ users: MemberProfile[] }>(`/workspaces/${workspaceId}/chat-directory`, undefined, abort.signal).then(directory => {
      const me = directory.users.find(item => item.id === user?.id);
      if (!me) return;
      setSaved(me);
      setDraft({ name: user?.display_name || '', title: me.title, about: me.about });
    }).catch(error => { if (!abort.signal.aborted) setLoadError(errorMessage(error)); });
    return () => abort.abort();
  }, [user?.id, user?.display_name]);
  if (!user) return null;
  const changed = saved && (draft.name.trim() !== (user.display_name ?? '') || draft.title.trim() !== saved.title || draft.about.trim() !== saved.about);
  const save = () => mutation.run(async () => {
    const next = await identityRequest<MemberProfile>(`/workspaces/${workspaceId}/my-profile`, { display_name: draft.name, title: draft.title, about: draft.about });
    setSaved(next);
    // The account menu reads the same session object.
    user.display_name = next.name;
    mutation.setStatus('Profile saved.');
  });
  const upload = (file: File) => mutation.run(async () => {
    if (file.size > PHOTO_LIMIT) throw new Error('Choose a photo of at most 2 MB.');
    const response = await fetch(`/api/identity/workspaces/${workspaceId}/my-profile/photo`, { method: 'POST', headers: { 'X-Ayati-Request': 'identity' }, body: file });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || 'Photo upload failed.');
    setSaved(result);
    mutation.setStatus('Photo updated.');
  });
  const removePhoto = () => mutation.run(async () => {
    setSaved(await identityRequest<MemberProfile>(`/workspaces/${workspaceId}/my-profile/photo/remove`, {}));
    mutation.setStatus('Photo removed.');
  });
  if (loadError) return <p className="ui-help" role="alert">{loadError}</p>;
  if (!saved) return <p className="ui-help" role="status">Loading your profile…</p>;
  return <form className="my-profile" onSubmit={event => { event.preventDefault(); void save(); }}>
    <fieldset className="ui-fieldset" disabled={mutation.busy}>
      <div className="my-profile-photo">
        <PersonFace person={{ id: user.id, name: draft.name || user.email, photo: saved.photo }} className="my-profile-face" />
        <div>
          <div className="my-profile-photo-actions">
            <Button onClick={() => photo.current?.click()}>{saved.photo ? 'Change photo' : 'Upload photo'}</Button>
            {saved.photo && <Button onClick={() => void removePhoto()}>Remove</Button>}
          </div>
          <p className="ui-help">PNG, JPEG or WebP, up to 2 MB. A square photo works best.</p>
          <input ref={photo} hidden type="file" accept="image/png,image/jpeg,image/webp" aria-label="Choose a profile photo"
            onChange={event => { const file = event.target.files?.[0]; event.target.value = ''; if (file) void upload(file); }} />
        </div>
      </div>
      <label className="ui-field">Display name<input name="display_name" required maxLength={80} autoComplete="name" value={draft.name} onChange={event => setDraft({ ...draft, name: event.target.value })} /></label>
      <label className="ui-field"><span>Job title <small>Shown next to your name</small></span><input name="title" maxLength={80} placeholder="e.g. Design lead" value={draft.title} onChange={event => setDraft({ ...draft, title: event.target.value })} /></label>
      <label className="ui-field"><span>About <small>{draft.about.length} / 200</small></span><textarea name="about" rows={3} maxLength={200} placeholder="What you work on and what people can ask you about." value={draft.about} onChange={event => setDraft({ ...draft, about: event.target.value })} /></label>
      <Button primary type="submit" disabled={!draft.name.trim() || !changed}>{mutation.busy ? 'Saving…' : 'Save profile'}</Button>
    </fieldset>
    <p className="ui-help" role="status">{mutation.status}</p>
  </form>;
}
