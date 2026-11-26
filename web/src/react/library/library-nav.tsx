import { useCallback } from 'react';
import { identityRequest, workspaceId, currentWorkspace } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { LibraryIcon } from './icon';
import { folderUrl, request } from './model';

/** Where people go in the Library. Everything else is a filter on the list. */
export const places = [
  { id: 'all', label: 'Home' },
  { id: 'shared', label: 'Shared with me' },
  { id: 'trash', label: 'Trash' },
];
/** Admins also see what agents couldn't answer, and everything in the Library. */
const adminPlaces = [
  { id: 'gaps', label: 'Unanswered' },
  { id: 'everything', label: 'Everything' },
];
export const filters = [
  { id: 'private', label: 'Private to me' },
  { id: 'starred', label: 'Starred' },
  { id: 'agents', label: 'Changed by an agent' },
  { id: 'review', label: 'Due for review' },
];
export const viewLabel = (id: string) =>
  [...places, ...adminPlaces, ...filters].find((option) => option.id === id)?.label ?? 'Library';

export type TreeFolder = { id: string; title: string; folder: string | null; editable: boolean };
/** Every folder the person was given, for the tree and for moving things. */
export const loadFolders = (signal?: AbortSignal) =>
  identityRequest<{ folders?: TreeFolder[] }>(
    `/workspaces/${workspaceId}/library-folders`,
    undefined,
    signal,
  );

/** What a dragged Library row carries: the item and its sharing revision. */
export const draggedItem = 'application/x-cuckoo-library-item';

/** The places and, under them, the folders as a tree. */
export function LibraryNav({
  view,
  folder,
  pick,
  refreshKey,
  moved,
}: {
  view: string;
  folder: string | null;
  pick: (view: string) => void;
  refreshKey: number;
  moved: () => void;
}) {
  const options = [...places, ...(currentWorkspace()?.role === 'Admin' ? adminPlaces : [])];
  return (
    <>
      <nav aria-label="Library views">
        {options.map((option) => (
          <Button
            key={option.id}
            aria-current={view === option.id && !folder ? 'page' : undefined}
            onClick={() => pick(option.id)}
          >
            <span>
              <LibraryIcon name={option.id} />
            </span>
            {option.label}
          </Button>
        ))}
      </nav>
      <FolderTree key={refreshKey} open={folder} moved={moved} />
    </>
  );
}

function FolderTree({ open, moved }: { open: string | null; moved: () => void }) {
  const load = useCallback(() => loadFolders(), []);
  const tree = useResource(load);
  const mutation = useMutation();
  // Dropping a row on a folder files it there, like Move.
  const drop = (target: TreeFolder, data: string) =>
    void mutation.run(async () => {
      const dragged = JSON.parse(data) as { id: string; revision: number };
      if (dragged.id === target.id) return;
      await request(`/${dragged.id}/move`, {
        request: crypto.randomUUID(),
        revision: dragged.revision,
        folder: target.id,
      });
      moved();
    });
  const folders = tree.data?.folders ?? [];
  if (folders.length === 0) return null;
  // A folder whose parent the person can't see is shown at the top.
  const known = new Set(folders.map((f) => f.id));
  const inside = (parent: string | null) =>
    folders.filter((f) => (f.folder && known.has(f.folder) ? f.folder : null) === parent);
  const level = (parent: string | null, depth: number) => {
    const here = inside(parent);
    if (here.length === 0 || depth > 5) return null;
    return (
      <ul>
        {here.map((f) => (
          <li key={f.id}>
            <a
              href={folderUrl(f.id)}
              aria-current={open === f.id ? 'page' : undefined}
              onDragOver={(event) => {
                if (f.editable && event.dataTransfer.types.includes(draggedItem))
                  event.preventDefault();
              }}
              onDrop={(event) => {
                const data = event.dataTransfer.getData(draggedItem);
                if (!f.editable || !data) return;
                event.preventDefault();
                drop(f, data);
              }}
            >
              {f.title}
            </a>
            {level(f.id, depth + 1)}
          </li>
        ))}
      </ul>
    );
  };
  return (
    <nav aria-label="Folders" className="library-tree">
      <span className="library-eyebrow">Folders</span>
      {mutation.status && <small role="alert">{mutation.status}</small>}
      {level(null, 0)}
    </nav>
  );
}

/** Filters on the list; choosing the active one clears it. */
export function LibraryFilters({ view, pick }: { view: string; pick: (view: string) => void }) {
  return (
    <div className="library-filters" role="group" aria-label="Filters">
      {filters.map((option) => (
        <Button
          key={option.id}
          aria-pressed={view === option.id}
          onClick={() => pick(view === option.id ? 'all' : option.id)}
        >
          {option.label}
        </Button>
      ))}
    </div>
  );
}
