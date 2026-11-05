import { useCallback, useEffect, useRef, useState } from 'react';
import { navigate } from '../../navigation';
import { workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { LibraryItem } from './item';
import { FolderHeader, NewFolderDialog } from './folders';
import {
  emptyDocument,
  folderUrl,
  itemUrl,
  request,
  bytesLabel,
  dateLabel,
  visibilityName,
  type Item,
  type Listing,
} from './model';
import { useUploads } from './uploads';
import { LibraryIcon } from './icon';
import './library.css';
const views = [
  { id: 'all', label: 'All items' },
  { id: 'private', label: 'Private' },
  { id: 'shared', label: 'Shared' },
  { id: 'starred', label: 'Starred' },
  { id: 'trash', label: 'Trash' },
];
export function LibraryPage({ href }: { href: string }) {
  const selected = new URL(href).searchParams.get('item');
  const folder = new URL(href).searchParams.get('folder');
  const [view, setView] = useState('all'),
    [search, setSearch] = useState(''),
    [query, setQuery] = useState(''),
    [offset, setOffset] = useState(0),
    [naming, setNaming] = useState(false);
  const [refreshKey, setRefreshKey] = useState(0);
  const upload = useUploads(),
    picker = useRef<HTMLInputElement>(null),
    mutation = useMutation();
  const creating = useRef<{ id: string; request: string; folder?: string }>(undefined);
  useEffect(() => {
    const timer = setTimeout(() => {
      setQuery(search.trim());
      setOffset(0);
    }, 250);
    return () => clearTimeout(timer);
  }, [search]);
  const load = useCallback(
    () =>
      workspaceId
        ? request<Listing>(
            `?view=${view}&q=${encodeURIComponent(query)}&folder=${encodeURIComponent(
              view === 'all' ? (folder ?? '') : '',
            )}&offset=${offset}`,
          )
        : Promise.resolve<Listing>({
            available: false,
            items: [],
            more: false,
            storage: 'unconfigured',
          }),
    [view, query, folder, offset, refreshKey, upload.version],
  );
  const listing = useResource(load);
  const changed = () => setRefreshKey((n) => n + 1);
  const create = () =>
    void mutation.run(async () => {
      if (creating.current?.folder !== (folder ?? undefined))
        creating.current = {
          id: crypto.randomUUID(),
          request: crypto.randomUUID(),
          folder: folder ?? undefined,
        };
      const item = await request<Item>('', {
        ...creating.current,
        title: 'Untitled page',
        revision: 0,
        document: emptyDocument,
      });
      creating.current = undefined;
      changed();
      navigate(itemUrl(item.id));
    });
  const filesEnabled = !!listing.data && listing.data.storage !== 'unconfigured';
  return (
    <div
      className="library-page"
      onDragOver={(event) => {
        if (filesEnabled && event.dataTransfer.types.includes('Files')) event.preventDefault();
      }}
      onDrop={(event) => {
        if (filesEnabled && event.dataTransfer.files.length) {
          event.preventDefault();
          upload.choose(event.dataTransfer.files);
        }
      }}
    >
      <input
        ref={picker}
        hidden
        multiple
        type="file"
        aria-label="Upload Library files"
        onChange={(event) => {
          if (event.target.files) upload.choose(event.target.files);
          event.target.value = '';
        }}
      />
      {selected && workspaceId ? (
        <LibraryItem
          key={selected}
          id={selected}
          sourceRevision={new URL(href).searchParams.get('source_revision')}
          changed={changed}
        />
      ) : (
        <>
          <header className="library-header">
            <div>
              <h1>Library</h1>
              <p>A home for your plans, files and shared knowledge.</p>
            </div>
            <div className="library-header-actions">
              <input
                type="search"
                aria-label="Search Library"
                placeholder="Search your library"
                value={search}
                onChange={(event) => setSearch(event.target.value)}
              />
              <Button
                disabled={!filesEnabled}
                title={
                  filesEnabled
                    ? 'Upload files up to 25 MiB each'
                    : 'File storage needs to be configured on the server'
                }
                onClick={() => picker.current?.click()}
              >
                ↑ Upload
              </Button>
              {!folder && (
                <Button disabled={!listing.data?.available} onClick={() => setNaming(true)}>
                  ＋ New folder
                </Button>
              )}
              <Button primary disabled={!listing.data?.available || mutation.busy} onClick={create}>
                ＋ New page
              </Button>
            </div>
          </header>
          <div className="library-layout">
            <aside className="library-sidebar">
              <span className="library-eyebrow">Browse</span>
              <nav aria-label="Library views">
                {views.map((option) => (
                  <Button
                    key={option.id}
                    aria-current={view === option.id ? 'page' : undefined}
                    onClick={() => {
                      setView(option.id);
                      setOffset(0);
                      if (folder) navigate(itemUrl());
                    }}
                  >
                    <span>
                      <LibraryIcon name={option.id} />
                    </span>
                    {option.label}
                  </Button>
                ))}
              </nav>
              <p>
                Start small.
                <br />
                New pages and files are private until you share them.
              </p>
              {listing.data?.storage === 'local' && <small>Local development file storage</small>}
            </aside>
            <main className="library-list">
              {folder && view === 'all' && !query ? (
                <FolderHeader key={folder} id={folder} changed={changed} />
              ) : (
                <header>
                  <div>
                    <h2>{views.find((v) => v.id === view)?.label}</h2>
                    <p>
                      {query
                        ? `Results for “${query}”`
                        : view === 'trash'
                          ? 'Restore items when you need them again.'
                          : 'Create something new, or pick up where you left off.'}
                    </p>
                  </div>
                  <Button onClick={() => void listing.refresh().catch(() => {})}>Refresh</Button>
                </header>
              )}
              {(listing.error || mutation.status) && (
                <p className="library-notice" role="alert">
                  {listing.error || mutation.status}
                </p>
              )}
              {!listing.data && !listing.error && <p role="status">Opening Library…</p>}
              {listing.data?.available === false && (
                <div className="library-empty">
                  <h2>Library needs a workspace database</h2>
                  <p>
                    This deployment uses the legacy local mode. Library is available with PostgreSQL
                    workspace identity.
                  </p>
                </div>
              )}
              {listing.data?.available && listing.data.items.length === 0 && (
                <div className="library-empty">
                  <span aria-hidden="true">▤</span>
                  <h2>
                    {query
                      ? 'No matching items'
                      : view === 'all'
                        ? folder
                          ? 'This folder is empty'
                          : 'Make room for your next idea'
                        : 'Nothing here yet'}
                  </h2>
                  <p>
                    {query
                      ? 'Try a different title or phrase.'
                      : folder
                        ? 'New pages you create here follow the folder’s sharing. Move existing items in from their page.'
                        : 'Write a plan, keep a useful reference, and choose who can use it.'}
                  </p>
                  {view === 'all' && !query && !folder && (
                    <Button primary disabled={mutation.busy} onClick={create}>
                      Create your first page
                    </Button>
                  )}
                </div>
              )}
              {!!listing.data?.items.length && (
                <div className="library-table" role="table" aria-label="Library items">
                  <div className="library-table-heading" role="row">
                    <span role="columnheader">Name</span>
                    <span role="columnheader">Shared with</span>
                    <span role="columnheader">Agent access</span>
                    <span role="columnheader">Updated</span>
                    <span role="columnheader">Actions</span>
                  </div>
                  {listing.data.items.map((item) => (
                    <div className="library-table-row" role="row" key={item.id}>
                      <div role="cell" className="library-name">
                        <span className={`library-type ${item.kind}`}>
                          <LibraryIcon name={item.kind} />
                        </span>
                        <div>
                          {view === 'trash' ? (
                            <strong>{item.title}</strong>
                          ) : (
                            <a
                              href={item.kind === 'folder' ? folderUrl(item.id) : itemUrl(item.id)}
                            >
                              {item.title}
                            </a>
                          )}
                          <small>
                            {item.kind === 'folder'
                              ? `Folder · ${item.item_count ?? 0} item${item.item_count === 1 ? '' : 's'}`
                              : item.kind === 'page'
                                ? 'Page'
                                : `${item.mime.split('/').at(-1)} · ${bytesLabel(item.size)}`}
                            {item.folder_title &&
                              item.folder !== folder &&
                              ` · in ${item.folder_title}`}
                          </small>
                        </div>
                      </div>
                      <span role="cell">{visibilityName(item.visibility)}</span>
                      <span role="cell">
                        {item.agent_count
                          ? `${item.agent_count} agent${item.agent_count === 1 ? '' : 's'}`
                          : 'No agents'}
                        {item.agent_count && item.kind !== 'folder' ? (
                          <small className="library-agent-readiness">
                            {item.readiness === 'ready'
                              ? 'Text ready'
                              : item.readiness === 'partial'
                                ? 'Limited text'
                                : 'Reading unavailable'}
                          </small>
                        ) : null}
                      </span>
                      <span role="cell">{dateLabel(item.updated)}</span>
                      <span role="cell">
                        {view === 'trash' ? (
                          item.manageable && (
                            <Button
                              disabled={mutation.busy}
                              onClick={() =>
                                void mutation.run(async () => {
                                  await request(`/${item.id}/action`, {
                                    request: crypto.randomUUID(),
                                    revision: item.revision,
                                    action: 'restore',
                                  });
                                  changed();
                                })
                              }
                            >
                              Restore
                            </Button>
                          )
                        ) : (
                          <Button
                            aria-label={
                              item.starred ? `Unstar ${item.title}` : `Star ${item.title}`
                            }
                            disabled={mutation.busy}
                            onClick={() =>
                              void mutation.run(async () => {
                                await request(`/${item.id}/action`, {
                                  request: crypto.randomUUID(),
                                  revision: item.revision,
                                  action: item.starred ? 'unstar' : 'star',
                                });
                                changed();
                              })
                            }
                          >
                            {item.starred ? '★' : '☆'}
                          </Button>
                        )}
                      </span>
                    </div>
                  ))}
                </div>
              )}
              {(offset > 0 || listing.data?.more) && (
                <footer className="library-pagination">
                  <Button
                    disabled={offset === 0}
                    onClick={() => setOffset((n) => Math.max(0, n - 50))}
                  >
                    Previous
                  </Button>
                  <span>Page {Math.floor(offset / 50) + 1}</span>
                  <Button disabled={!listing.data?.more} onClick={() => setOffset((n) => n + 50)}>
                    Next
                  </Button>
                </footer>
              )}
              {listing.data?.available && (
                <div className="library-drop-hint">
                  {filesEnabled
                    ? 'Drop files here to upload · up to 25 MiB each · private by default'
                    : 'Pages are ready. File uploads become available when server storage is configured.'}
                </div>
              )}
            </main>
          </div>
          {naming && <NewFolderDialog close={() => setNaming(false)} />}
        </>
      )}
    </div>
  );
}
