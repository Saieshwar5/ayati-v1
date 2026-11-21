import { useEffect, useId, useRef, useState, type DragEvent } from 'react';
import { currentWorkspace, identityRequest, workspaceId, workspaceSession } from '../../workspace-context';
import { PersonFace } from '../faces';
import { errorMessage, useMutation } from '../requests';
import { Button } from '../ui';
import { applyPresence, type Presence } from '../chat/presence';
import { ProfilePreview } from './profile-preview';
import { StatusAndPresence } from './status-and-presence';
import { SaveBar } from './save-bar';
import { usePreferences } from './use-preferences';

type MemberProfile = { id: string; name: string; title: string; about: string; handles?: string; photo: string | null };
type Draft = { name: string; title: string; about: string; handles: string };
const PHOTO_LIMIT = 2 * 1024 * 1024;
const PHOTO_TYPES = ['image/png', 'image/jpeg', 'image/webp'];
const ABOUT_LIMIT = 200;

/** What colleagues see on your profile card and next to your messages. The card on
 * the right is that profile, drawn from the draft as you type. */
export function MyProfile() {
  const user = workspaceSession?.user;
  const [saved, setSaved] = useState<MemberProfile>();
  const [draft, setDraft] = useState<Draft>({ name: '', title: '', about: '', handles: '' });
  const [loadError, setLoadError] = useState('');
  const mutation = useMutation();
  const preferences = usePreferences();
  const photo = useRef<HTMLInputElement>(null);
  const form = useId();
  const asSaved = (profile: MemberProfile): Draft => ({ name: user?.display_name || '', title: profile.title, about: profile.about, handles: profile.handles ?? '' });
  useEffect(() => {
    const abort = new AbortController();
    identityRequest<{ users: MemberProfile[]; presence?: Presence[] }>(`/workspaces/${workspaceId}/chat-directory`, undefined, abort.signal).then(directory => {
      const me = directory.users.find(item => item.id === user?.id);
      if (!me) return;
      // Your own status and online dot, for the card.
      applyPresence(directory.presence);
      setSaved(me);
      setDraft({ name: user?.display_name || '', title: me.title, about: me.about, handles: me.handles ?? '' });
    }).catch(error => { if (!abort.signal.aborted) setLoadError(errorMessage(error)); });
    return () => abort.abort();
  }, [user?.id, user?.display_name]);
  if (!user) return null;
  const changed = saved && (draft.name.trim() !== (user.display_name ?? '') || draft.title.trim() !== saved.title || draft.about.trim() !== saved.about || draft.handles.trim() !== (saved.handles ?? ''));
  const edit = (change: Partial<Draft>) => setDraft(old => ({ ...old, ...change }));
  const save = () => mutation.run(async () => {
    const next = await identityRequest<MemberProfile>(`/workspaces/${workspaceId}/my-profile`, { display_name: draft.name, title: draft.title, about: draft.about, handles: draft.handles });
    setSaved(next);
    // The account menu reads the same session object.
    user.display_name = next.name;
    mutation.setStatus('Profile saved.');
  });
  const upload = (file: File) => mutation.run(async () => {
    if (!PHOTO_TYPES.includes(file.type)) throw new Error('Choose a PNG, JPEG or WebP image.');
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
  const drop = (event: DragEvent) => {
    event.preventDefault();
    const file = event.dataTransfer.files?.[0];
    if (file) void upload(file);
  };
  if (loadError) return <p className="ui-help" role="alert">{loadError}</p>;
  if (!saved) return <p className="ui-help" role="status">Loading your profile…</p>;
  const role = currentWorkspace()?.role ?? 'Member';
  const face = { id: user.id, name: draft.name || user.email, photo: saved.photo };
  return <div className="my-profile-columns">
    <div className="my-profile-edit">
      <form id={form} className="my-profile" onSubmit={event => { event.preventDefault(); if (draft.name.trim() && changed) void save(); }}>
        <fieldset className="ui-fieldset" disabled={mutation.busy}>
          <div className="my-profile-photo">
            <button type="button" className="my-profile-photo-button" aria-label="Change photo" onClick={() => photo.current?.click()}
              onDragOver={event => event.preventDefault()} onDrop={drop}>
              <PersonFace person={face} className="my-profile-face" />
              <span className="my-profile-camera" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8"><path d="M4 8h3l2-3h6l2 3h3v11H4ZM12 16a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z" /></svg></span>
            </button>
            <div className="my-profile-who">
              <strong>{draft.name || user.email}</strong>
              <span>{[draft.title.trim(), role].filter(Boolean).join(' · ')}</span>
              <p className="ui-help">Click the photo, or drop an image on it. PNG, JPEG or WebP, up to 2 MB.</p>
              {saved.photo && <Button onClick={() => void removePhoto()}>Remove photo</Button>}
            </div>
            <input ref={photo} hidden type="file" accept={PHOTO_TYPES.join(',')} aria-label="Choose a profile photo"
              onChange={event => { const file = event.target.files?.[0]; event.target.value = ''; if (file) void upload(file); }} />
          </div>
          <section className="my-profile-group" aria-labelledby={`${form}-about`}><h3 id={`${form}-about`}>About you</h3>
            <div className="my-profile-pair">
              <label className="ui-field">Name<input name="display_name" required maxLength={80} autoComplete="name" value={draft.name} onChange={event => edit({ name: event.target.value })} /></label>
              <label className="ui-field">Job title<input name="title" maxLength={80} placeholder="e.g. Design lead" value={draft.title} onChange={event => edit({ title: event.target.value })} /></label>
            </div>
            <label className="ui-field"><span className="my-profile-counted">About <small>{draft.about.length} / {ABOUT_LIMIT}</small></span>
              <textarea name="about" rows={3} maxLength={ABOUT_LIMIT} placeholder="What you work on and what people can ask you about." value={draft.about} onChange={event => edit({ about: event.target.value })} /></label>
          </section>
          <section className="my-profile-group" aria-labelledby={`${form}-agents`}><h3 id={`${form}-agents`}>For agents</h3>
            <label className="ui-field">What you handle<input name="handles" maxLength={160} placeholder="e.g. Prices, discounts, quotes over ₹50,000" value={draft.handles} onChange={event => edit({ handles: event.target.value })} /></label>
            <p className="ui-help">One line. Agents read it to decide whom to ask, and colleagues see it on your profile.</p>
          </section>
        </fieldset>
      </form>
      <StatusAndPresence userId={user.id} preferences={preferences} />
      <section className="my-profile-group" aria-labelledby={`${form}-account`}><h3 id={`${form}-account`}>Account</h3>
        <dl className="my-profile-facts">
          <div><dt>Sign-in email</dt><dd>{user.email}</dd></div>
          <div><dt>Role in this workspace</dt><dd>{role}</dd></div>
        </dl>
        <p className="ui-help">Your email comes from how you sign in, so it can’t be changed here.</p>
      </section>
      {changed && <SaveBar form={form} busy={mutation.busy} disabled={!draft.name.trim()} discard={() => { setDraft(asSaved(saved)); mutation.setStatus(''); }} />}
      <p className="ui-status" role="status">{mutation.status}</p>
    </div>
    <ProfilePreview person={face} title={draft.title.trim() || (role === 'Admin' ? 'Workspace admin' : 'Member')} about={draft.about.trim()} handles={draft.handles.trim()}
      email={user.email} hideOnline={preferences.data?.show_online === false} />
  </div>;
}
