import { useCallback, useRef, useState } from 'react';
import { identityRequest, workspaceId } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { appColour, appsRequest, openUrl, type AppRequestRow, type CompanySystem } from './apps-model';

export function LinkIcon() {
  return (
    <svg viewBox="0 0 24 24" fill="none" stroke="#fff" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />
    </svg>
  );
}

/** One id per attempt, kept while the same thing is retried. */
function useAttemptId() {
  const attempt = useRef<{ key: string; id: string }>(undefined);
  return (key: string) => {
    if (attempt.current?.key !== key) attempt.current = { key, id: crypto.randomUUID() };
    return attempt.current.id;
  };
}

/** The side panel for a company system: open it, ask for access, or manage it. */
export function SystemPanel({ app, canManage, changed, edit }: { app: CompanySystem; canManage: boolean; changed: () => void; edit: () => void }) {
  const [asking, setAsking] = useState(false);
  const mutation = useMutation();
  return (
    <aside className="app-panel" aria-label={app.name}>
      <span className="app-icon big" style={{ background: appColour(app.id) }}><LinkIcon /></span>
      <div><h2>{app.name}</h2><p className="app-line">Your system{app.description ? ` · ${app.description}` : ''}</p></div>
      {app.can_open ? (
        <a className="ui-button primary" href={openUrl(app.id)} target="_blank" rel="noopener noreferrer">Open {app.name}</a>
      ) : app.requested ? (
        <p className="app-callout">You asked for access. {app.owner_name ?? 'An admin'} decides.</p>
      ) : (
        <Button primary onClick={() => setAsking(true)}>Request access</Button>
      )}
      <section aria-label="Who can open it"><h3>Who can open it</h3>
        <p>{app.audience === 'everyone' ? 'Everyone in the workspace' : app.people ? (app.people.length ? app.people.map(p => p.name).join(', ') : 'Only its owner and admins') : 'Chosen people'}</p>
        {app.owner_name && <p className="app-line">Owner: {app.owner_name}</p>}</section>
      <section aria-label="How it opens"><h3>How it opens</h3>
        <p>Through Cuckoo, which checks who may open it first. The app keeps its own sign-in.</p></section>
      {canManage && <div className="apps-row">
        <Button onClick={edit}>Manage</Button>
        <Button disabled={mutation.busy} onClick={() => void mutation.run(async () => {
          if (!confirm(`Remove ${app.name}? Nobody will be able to open it from Cuckoo.`)) return;
          await appsRequest(`/systems/${encodeURIComponent(app.id)}/remove`, {});
          changed();
        })}>Remove</Button>
      </div>}
      {mutation.status && <p role="alert">{mutation.status}</p>}
      {asking && <RequestDialog app={app} close={() => setAsking(false)} sent={changed} />}
    </aside>
  );
}

/** Ask for access to an app, or (no app) suggest one the company should add. A
 * catalog service (`suggest`) is requested as a suggestion under its own name. */
export function RequestDialog({ app, suggest, close, sent }: { app?: CompanySystem; suggest?: string; close: () => void; sent: () => void }) {
  const mutation = useMutation();
  const attempt = useAttemptId();
  const [name, setName] = useState(suggest ?? ''), [url, setUrl] = useState(''), [reason, setReason] = useState('');
  const send = () => void mutation.run(async () => {
    const body = app ? { app: app.id, reason } : { name, url, reason };
    await appsRequest('/requests', { id: attempt(JSON.stringify(body)), ...body });
    sent();
    close();
  });
  return (
    <Dialog title={app ? `Request access to ${app.name}` : suggest ? `Request ${suggest}` : 'Suggest an app'} close={close} busy={mutation.busy} status={mutation.status}
      footer={<><Button disabled={mutation.busy} onClick={close}>Cancel</Button>
        <Button primary disabled={mutation.busy || (!app && !name.trim())} onClick={send}>{app || suggest ? 'Send request' : 'Send suggestion'}</Button></>}>
      <p>{app ? `${app.owner_name ?? 'An admin'} decides. You'll be able to open it from Apps once approved.`
        : suggest ? `Admins see your request on the Apps page and decide whether to connect ${suggest}.` : 'Admins see your suggestion and decide whether to add it.'}</p>
      {!app && !suggest && <>
        <label className="ui-field">App name<input aria-label="App name" maxLength={80} value={name} onChange={e => setName(e.target.value)} /></label>
        <label className="ui-field">Address <small>Optional, starting with https://</small><input aria-label="Address" maxLength={2000} value={url} onChange={e => setUrl(e.target.value)} /></label>
      </>}
      <label className="ui-field">Why <small>Optional</small><textarea aria-label="Why" rows={3} maxLength={500} value={reason} onChange={e => setReason(e.target.value)} /></label>
    </Dialog>
  );
}

type Person = { id: string; name: string };
/** Admins add or change a company system: its address and who can open it. */
export function SystemForm({ app, close, saved }: { app?: CompanySystem; close: () => void; saved: () => void }) {
  const loadPeople = useCallback(() => identityRequest<{ users: Person[] }>(`/workspaces/${workspaceId}/chat-directory`), []);
  const people = useResource(loadPeople);
  const mutation = useMutation();
  const id = useRef(app?.id ?? crypto.randomUUID());
  const [name, setName] = useState(app?.name ?? ''), [description, setDescription] = useState(app?.description ?? '');
  const [url, setUrl] = useState(app?.url ?? 'https://'), [audience, setAudience] = useState(app?.audience ?? 'everyone');
  const [chosen, setChosen] = useState<string[]>(app?.people?.map(p => p.id) ?? []), [owner, setOwner] = useState(app?.owner ?? '');
  const save = () => void mutation.run(async () => {
    await appsRequest('/systems', { id: id.current, name, description, url, audience, people: audience === 'selected' ? chosen : [], owner: owner || null, revision: app?.revision ?? 0 });
    saved();
    close();
  });
  return (
    <Dialog drawer title={app ? `Manage ${app.name}` : "Add your company's app"} close={close} busy={mutation.busy} status={mutation.status || people.error}
      footer={<><Button disabled={mutation.busy} onClick={close}>Cancel</Button>
        <Button primary disabled={mutation.busy || !name.trim() || !url.startsWith('https://')} onClick={save}>{app ? 'Save changes' : 'Add app'}</Button></>}>
      <p>A link to a system your company already runs, such as an order portal. It opens through Cuckoo for the people you choose and keeps its own sign-in.</p>
      <label className="ui-field">Name<input aria-label="Name" maxLength={80} value={name} onChange={e => setName(e.target.value)} /></label>
      <label className="ui-field">What it is for <small>Optional</small><input aria-label="What it is for" maxLength={200} value={description} onChange={e => setDescription(e.target.value)} /></label>
      <label className="ui-field">Address<input aria-label="Address" maxLength={2000} value={url} onChange={e => setUrl(e.target.value)} /></label>
      <label className="ui-field">Who can open it<select aria-label="Who can open it" value={audience} onChange={e => setAudience(e.target.value as CompanySystem['audience'])}>
        <option value="everyone">Everyone in the workspace</option><option value="selected">Chosen people</option></select></label>
      {audience === 'selected' && <fieldset className="apps-people"><legend>People</legend>
        {people.data?.users.map(person => <label key={person.id} className="ui-check"><input type="checkbox" checked={chosen.includes(person.id)}
          onChange={e => setChosen(old => e.target.checked ? [...old, person.id] : old.filter(p => p !== person.id))} />{person.name}</label>)}</fieldset>}
      <label className="ui-field">Owner <small>Decides access requests</small><select aria-label="Owner" value={owner} onChange={e => setOwner(e.target.value)}>
        <option value="">No owner (admins decide)</option>
        {people.data?.users.map(person => <option key={person.id} value={person.id}>{person.name}</option>)}</select></label>
    </Dialog>
  );
}

/** Waiting access requests and suggestions for the person who decides them. */
export function RequestsDrawer({ close, changed }: { close: () => void; changed: () => void }) {
  const load = useCallback(() => appsRequest<{ requests: AppRequestRow[] }>('/requests'), []);
  const requests = useResource(load);
  const mutation = useMutation();
  const decide = (id: string, action: 'approve' | 'decline') => void mutation.run(async () => {
    await appsRequest(`/requests/${encodeURIComponent(id)}`, { action });
    await requests.refresh();
    changed();
  });
  return (
    <Dialog drawer title="Requests" close={close} busy={mutation.busy} status={mutation.status || requests.error}>
      {requests.data && !requests.data.requests.length && <p className="ui-empty">Nothing waiting.</p>}
      <ul className="apps-requests">{requests.data?.requests.map(r => <li key={r.id}>
        <p><strong>{r.requester}</strong> {r.kind === 'access' ? <>asks to open <strong>{r.app_name}</strong></> : <>suggests <strong>{r.name}</strong>{r.url && <> ({r.url})</>}</>}</p>
        {r.reason && <p className="app-line">“{r.reason}”</p>}
        <div className="apps-row">
          <Button primary disabled={mutation.busy} onClick={() => decide(r.id, 'approve')}>{r.kind === 'access' ? 'Approve' : 'Accept'}</Button>
          <Button disabled={mutation.busy} onClick={() => decide(r.id, 'decline')}>Decline</Button>
        </div>
      </li>)}</ul>
    </Dialog>
  );
}
