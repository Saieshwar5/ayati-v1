import { useCallback, useRef, useState } from 'react';
import { navigate } from '../../navigation';
import { identityRequest, workspaceId } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { ShareDialog, useAttempt } from './dialogs';
import { KeepingControl } from './keeping';
import { MeaningControl } from './meaning';
import { FolderWatch } from './watching';
import { loadFolders } from './library-nav';
import {
  folderUrl,
  itemUrl,
  request,
  useForName,
  visibilityName,
  type Item,
} from './model';

/** The open folder: its name, whose it is, and its sharing, which its items follow. */
export function FolderHeader({ id, changed }: { id: string; changed: () => void }) {
  const load = useCallback(() => request<Item>(`/${id}`), [id]);
  const folder = useResource(load);
  const mutation = useMutation();
  const [sharing, setSharing] = useState(false);
  const item = folder.data;
  // A subfolder follows the folder it is in until it is shared separately.
  const follows = Boolean(item?.folder && item.source !== item.id);
  const [moving, setMoving] = useState(false);
  const refresh = () => {
    void folder.refresh().catch(() => {});
    changed();
  };
  return (
    <header className="library-folder-header">
      <div>
        <nav aria-label="Folder path" className="library-path">
          <a href={itemUrl()}>Library</a>
          <span aria-hidden="true">›</span>
          {item?.folder && (
            <>
              <a href={folderUrl(item.folder)}>{item.folder_title ?? 'Folder'}</a>
              <span aria-hidden="true">›</span>
            </>
          )}
          <span>{item?.title ?? 'Folder'}</span>
        </nav>
        <h2>{item?.title ?? 'Opening folder…'}</h2>
        {item && (
          <p>
            {visibilityName(item.visibility)}
            {item.agents.length > 0 &&
              ` · ${item.agents.length} agent${item.agents.length === 1 ? '' : 's'} (${[
                ...new Set(item.agents.map((g) => useForName(g.use_for).toLowerCase())),
              ].join(', ')})`}
            .{' '}
            {follows
              ? `This folder follows the sharing of ${item.folder_title ?? 'its folder'}.`
              : "Everything here follows the folder's sharing."}
          </p>
        )}
        {item && <KeepingControl item={item} />}
        {item && <MeaningControl item={item} />}
        {item && <FolderWatch item={item} />}
        {(folder.error || mutation.status) && (
          <p className="library-notice" role="alert">
            {folder.error || mutation.status}
          </p>
        )}
      </div>
      {item?.manageable && (
        <div className="library-folder-actions">
          <Button onClick={() => setSharing(true)}>
            {follows ? 'Share separately' : 'Share folder'}
          </Button>
          {item.folder && !follows && (
            <Button
              disabled={mutation.busy}
              onClick={() =>
                void mutation.run(async () => {
                  await request(`/${id}/action`, {
                    request: crypto.randomUUID(),
                    revision: item.revision,
                    action: 'follow_folder',
                  });
                  refresh();
                })
              }
            >
              Follow {item.folder_title ?? 'folder'}
            </Button>
          )}
          <Button onClick={() => setMoving(true)}>Move</Button>
          <Button
            disabled={mutation.busy}
            onClick={() =>
              void mutation.run(async () => {
                if (!confirm('Move this empty folder to Trash?')) return;
                await request(`/${id}/action`, {
                  request: crypto.randomUUID(),
                  revision: item.revision,
                  action: 'trash',
                });
                changed();
                navigate(item.folder ? folderUrl(item.folder) : itemUrl());
              })
            }
          >
            Trash folder
          </Button>
        </div>
      )}
      {sharing && item && (
        <ShareDialog item={item} close={() => setSharing(false)} changed={refresh} />
      )}
      {moving && item && (
        <MoveDialog item={item} close={() => setMoving(false)} changed={refresh} />
      )}
    </header>
  );
}

export function NewFolderDialog({ folder, close }: { folder?: string; close: () => void }) {
  const mutation = useMutation();
  const [title, setTitle] = useState('');
  // One folder identity per attempt, so a retried create never makes two folders.
  const attempt = useRef<{ id: string; request: string; title: string; folder?: string }>(
    undefined,
  );
  const create = () =>
    void mutation.run(async () => {
      if (attempt.current?.title !== title.trim())
        attempt.current = {
          id: crypto.randomUUID(),
          request: crypto.randomUUID(),
          title: title.trim(),
          folder,
        };
      await identityRequest(`/workspaces/${workspaceId}/library-folders`, attempt.current);
      navigate(folderUrl(attempt.current.id));
      close();
    });
  return (
    <Dialog
      title="New folder"
      close={close}
      busy={mutation.busy}
      status={mutation.status}
      footer={
        <>
          <Button disabled={mutation.busy} onClick={close}>
            Cancel
          </Button>
          <Button primary disabled={mutation.busy || !title.trim()} onClick={create}>
            Create folder
          </Button>
        </>
      }
    >
      <p>
        Folders keep related pages and files together. Share the folder once and everything in it
        follows.{folder ? ' A folder made here follows this folder until you share it separately.' : ''}
      </p>
      <label className="library-field">
        Folder name
        <input
          aria-label="Folder name"
          autoFocus
          maxLength={180}
          value={title}
          placeholder="Sales, Policies, HR…"
          onChange={(event) => setTitle(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && title.trim() && !mutation.busy) create();
          }}
        />
      </label>
    </Dialog>
  );
}
/** File an item into a folder the person can edit, or take it out of its folder. */
export function MoveDialog({
  item,
  close,
  changed,
}: {
  item: Item;
  close: () => void;
  changed: () => void;
}) {
  // Any folder the person can add to, at any depth.
  const load = useCallback(() => loadFolders(), []);
  const listing = useResource(load);
  const mutation = useMutation();
  const withRequest = useAttempt();
  const [target, setTarget] = useState(item.folder ?? '');
  const folders =
    listing.data?.folders?.filter((f) => f.editable && f.id !== item.id) ?? [];
  return (
    <Dialog
      title="Move to folder"
      close={close}
      busy={mutation.busy}
      status={mutation.status || listing.error}
      footer={
        <>
          <Button disabled={mutation.busy} onClick={close}>
            Cancel
          </Button>
          <Button
            primary
            disabled={mutation.busy || !listing.data || target === (item.folder ?? '')}
            onClick={() =>
              void mutation.run(async () => {
                await request(
                  `/${item.id}/move`,
                  withRequest({
                    revision: item.access_revision,
                    folder: target || null,
                  }),
                );
                changed();
                close();
              })
            }
          >
            Move
          </Button>
        </>
      }
    >
      <p>
        In a folder, <strong>{item.title}</strong> follows the folder's sharing and its own sharing
        is removed. Taken out of a folder, it is private to you again.
      </p>
      <label className="library-field">
        Folder
        <select
          aria-label="Folder"
          value={target}
          onChange={(event) => setTarget(event.target.value)}
        >
          <option value="">Not in a folder</option>
          {folders.map((folder) => (
            <option key={folder.id} value={folder.id}>
              {folder.title}
            </option>
          ))}
        </select>
      </label>
      {listing.data && folders.length === 0 && (
        <p className="library-muted">There are no folders you can add to yet.</p>
      )}
    </Dialog>
  );
}
