import { useCallback, useState, type FormEvent } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { MAX_ACTIONS, loadLibraryActions, saveAddedApp, searchLibrary, type AddedFact, type Effect, type LibraryAction, type LibraryApp } from '../apps/added-apps-model';
import './add-apps.css';

/** The library app being reviewed: a new one, or one the admins added before. */
export type Reviewing = { app: string; name: string; added?: AddedFact; connections?: number };

const tags: Record<Exclude<LibraryApp['state'], 'addable'>, string> = {
  builtin: 'Already one of Cuckoo’s apps', added: 'Already added', unsupported: 'Not supported yet',
};

/** Settings › Connections › Add more apps (schema 135), for admins: search the
 * library, then choose the actions agents may use and say which only read.
 * `editing` opens straight on an added app's actions. */
export function AddApps({ editing, close, done }: { editing?: Reviewing; close: () => void; done: (status: string) => void }) {
  const [reviewing, review] = useState(editing);
  if (reviewing) return <ReviewActions key={reviewing.app} reviewing={reviewing} close={close} done={done} back={editing ? undefined : () => review(undefined)} />;
  return <SearchLibrary close={close} choose={review} />;
}

function SearchLibrary({ close, choose }: { close: () => void; choose: (app: Reviewing) => void }) {
  const [text, setText] = useState('');
  const [query, setQuery] = useState('');
  const found = useResource(useCallback(() => searchLibrary(query), [query]));
  const search = (event: FormEvent) => { event.preventDefault(); setQuery(text.trim()); };
  const apps = found.data?.apps ?? [];
  return <Dialog title="Add more apps" close={close} footer={<Button onClick={close}>Close</Button>}>
    <div className="add-apps">
      <p>Search the app library and add an app to this workspace. You choose what agents may do with it.</p>
      <form className="add-apps-search" role="search" onSubmit={search}>
        <input type="search" className="ui-search" aria-label="Search the app library" placeholder="Search apps, such as HubSpot" maxLength={60}
          value={text} onChange={event => setText(event.target.value)} />
        <Button type="submit">Search</Button>
      </form>
      {found.error && <p role="alert">{found.error} <Button onClick={() => void found.refresh().catch(() => {})}>Retry</Button></p>}
      {!found.data && !found.error && <p role="status">Loading apps…</p>}
      {found.data && !apps.length && <p className="ui-empty">No app matches “{query}”. Try another name.</p>}
      {apps.length > 0 && <ul className="add-apps-list" aria-label="Library apps">{apps.map(app => <li key={app.app}>
        <span><strong>{app.name}</strong><small>{[app.description, app.actions > 0 && `${app.actions} action${app.actions === 1 ? '' : 's'}`].filter(Boolean).join(' · ')}</small>
          {app.state === 'unsupported' && app.reason && <small>{app.reason}</small>}</span>
        {app.state === 'addable'
          ? <Button primary aria-label={`Choose actions for ${app.name}`} onClick={() => choose({ app: app.app, name: app.name })}>Choose actions</Button>
          : <span className="add-apps-tag">{tags[app.state]}</span>}
      </li>)}</ul>}
    </div>
  </Dialog>;
}

type Picked = Map<string, { title: string; effect: Effect }>;

function ReviewActions({ reviewing, close, back, done }: { reviewing: Reviewing; close: () => void; back?: () => void; done: (status: string) => void }) {
  const { app, name, added } = reviewing;
  const [text, setText] = useState('');
  const [query, setQuery] = useState('');
  const first = useResource(useCallback(() => loadLibraryActions(app, query), [app, query]));
  // Later pages of the same search; a new search starts over.
  const [more, setMore] = useState<{ query: string; actions: LibraryAction[]; next: string | null }>();
  const [picked, setPicked] = useState<Picked>(() => new Map(added?.actions.map(a => [a.tool, { title: a.title, effect: a.effect }])));
  const paging = useMutation();
  const saving = useMutation();
  const extra = more?.query === query ? more : undefined;
  const actions = [...(first.data?.actions ?? []), ...(extra?.actions ?? [])];
  const next = extra ? extra.next : first.data?.next ?? null;
  const search = (event: FormEvent) => { event.preventDefault(); setQuery(text.trim()); };
  const showMore = () => void paging.run(async () => {
    if (!next) return;
    const page = await loadLibraryActions(app, query, next);
    setMore({ query, actions: [...(extra?.actions ?? []), ...page.actions], next: page.next });
  });
  const toggle = (action: LibraryAction, on: boolean) => setPicked(old => {
    const copy = new Map(old);
    if (on) copy.set(action.tool, { title: action.title, effect: action.effect ?? 'write' }); else copy.delete(action.tool);
    return copy;
  });
  const setEffect = (tool: string, effect: Effect) => setPicked(old => {
    const copy = new Map(old), chosen = copy.get(tool);
    if (chosen) copy.set(tool, { ...chosen, effect });
    return copy;
  });
  const full = picked.size >= MAX_ACTIONS;
  const connections = reviewing.connections ?? 0;
  const save = () => void saving.run(async () => {
    const saved = await saveAddedApp({ app, actions: [...picked].map(([tool, chosen]) => ({ tool, effect: chosen.effect })), revision: added?.revision ?? 0 });
    done(added ? `${saved.name} was changed.${saved.ended ? ` ${saved.ended} connection${saved.ended === 1 ? '' : 's'} ended; their owners connect again.` : ''}`
      : `${saved.name} was added and is allowed. People can add it to their agents now.`);
  });
  // Chosen actions that aren't in the list shown (another search, or a later page).
  const hidden = [...picked].filter(([tool]) => !actions.some(action => action.tool === tool));
  return <Dialog title={added ? `Change ${name}` : `Add ${name}`} close={close} back={back} busy={saving.busy} status={saving.status}
    footer={<><Button disabled={saving.busy} onClick={close}>Cancel</Button>
      <Button primary disabled={saving.busy || !picked.size} onClick={save}>{saving.busy ? 'Saving…' : added ? 'Save changes' : `Add ${name}`}</Button></>}>
    <div className="add-apps">
      <p>Choose what agents may do with {name}. An action that <strong>changes</strong> something asks a person first; one that only <strong>reads</strong> does not.</p>
      {added && <p role="note">{connections > 0
        ? `Saving ends ${connections} connection${connections === 1 ? '' : 's'} of ${name}. Their owners connect it again to get the new actions.`
        : `People who connect ${name} after you save get these actions.`}</p>}
      <form className="add-apps-search" role="search" onSubmit={search}>
        <input type="search" className="ui-search" aria-label={`Search ${name}’s actions`} placeholder={first.data ? `Search ${first.data.total} actions` : 'Search actions'} maxLength={60}
          value={text} onChange={event => setText(event.target.value)} />
        <Button type="submit">Search</Button>
      </form>
      <p className="add-apps-count" role="status">{picked.size} of {MAX_ACTIONS} chosen{full ? '. Remove one to choose another.' : ''}</p>
      {first.error && <p role="alert">{first.error} <Button onClick={() => void first.refresh().catch(() => {})}>Retry</Button></p>}
      {!first.data && !first.error && <p role="status">Loading actions…</p>}
      {first.data && !actions.length && <p className="ui-empty">No action matches “{query}”.</p>}
      {actions.length > 0 && <ul className="add-apps-list" aria-label={`${name} actions`}>{actions.map(action => {
        const chosen = picked.get(action.tool);
        return <li key={action.tool} className={action.available ? undefined : 'unavailable'}>
          <label className="add-apps-pick">
            <input type="checkbox" checked={!!chosen} disabled={!action.available || (full && !chosen)} onChange={event => toggle(action, event.target.checked)} />
            <span><strong>{action.title}</strong><small>{action.available ? action.description : `Not available. ${action.reason ?? ''}`}</small></span>
          </label>
          {chosen && <select aria-label={`What ${action.title} does`} value={chosen.effect} onChange={event => setEffect(action.tool, event.target.value as Effect)}>
            <option value="read" disabled={!action.may_read}>Reads</option>
            <option value="write">Changes · asks first</option>
          </select>}
        </li>;
      })}</ul>}
      {next && <Button className="add-apps-more" disabled={paging.busy} onClick={showMore}>{paging.busy ? 'Loading…' : 'Show more actions'}</Button>}
      {paging.status && <p role="alert">{paging.status}</p>}
      {hidden.length > 0 && <><p className="add-apps-count">Also chosen</p>
        <ul className="add-apps-list" aria-label="Also chosen">{hidden.map(([tool, chosen]) => <li key={tool}>
          <span><strong>{chosen.title}</strong><small>{chosen.effect === 'read' ? 'Reads' : 'Changes · asks first'}</small></span>
          <Button aria-label={`Remove ${chosen.title}`} onClick={() => setPicked(old => { const copy = new Map(old); copy.delete(tool); return copy; })}>Remove</Button>
        </li>)}</ul></>}
    </div>
  </Dialog>;
}

