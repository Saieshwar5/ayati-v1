import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react';
import { identityRequest, storageScope, workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { bytesLabel } from './model';
import { errorMessage } from '../requests';
type Upload = {
  id: string;
  request: string;
  title: string;
  size: number;
  checksum: string;
  state: string;
  progress: number;
  error?: string;
};
type Queue = { choose: (files: FileList | File[]) => void; version: number };
const Context = createContext<Queue>({ choose: () => {}, version: 0 });
export const useUploads = () => useContext(Context);
const uploadBase = `/workspaces/${workspaceId}/library-uploads`;
export function LibraryUploads({ children }: { children: ReactNode }) {
  const [tasks, setTasks] = useState<Upload[]>([]),
    [version, setVersion] = useState(0);
  const [notice, setNotice] = useState('');
  const tasksRef = useRef<Upload[]>([]),
    files = useRef(new Map<string, File>());
  const active = useRef(new Map<string, XMLHttpRequest>()),
    alive = useRef(false),
    serial = useRef(false);
  const cancelled = useRef(new Set<string>());
  const picker = useRef<HTMLInputElement>(null),
    resuming = useRef<Upload | undefined>(undefined);
  const key = storageScope('cuckoo.library.uploads');
  const update = (id: string, change: Partial<Upload>) => {
    if (!alive.current) return;
    tasksRef.current = tasksRef.current.map((t) => (t.id === id ? { ...t, ...change } : t));
    setTasks([...tasksRef.current]);
    try {
      localStorage.setItem(
        key,
        JSON.stringify(tasksRef.current.filter((t) => !['stored', 'cancelled'].includes(t.state))),
      );
    } catch {
      /* Files can still finish without optional session recovery. */
    }
  };
  useEffect(() => {
    alive.current = true;
    try {
      const raw = JSON.parse(localStorage.getItem(key) || '[]');
      if (Array.isArray(raw))
        tasksRef.current = raw
          .filter(
            (t) =>
              typeof t.id === 'string' &&
              typeof t.checksum === 'string' &&
              typeof t.title === 'string' &&
              t.size > 0 &&
              t.size <= 25 * 1024 * 1024,
          )
          .slice(0, 20)
          .map((t) => ({ ...t, state: 'interrupted', progress: 0 }));
      setTasks([...tasksRef.current]);
    } catch {
      /* Ignore malformed optional recovery data. */
    }
    return () => {
      alive.current = false;
      for (const xhr of active.current.values()) xhr.abort();
      active.current.clear();
      files.current.clear();
    };
  }, [key]);
  const transfer = (task: Upload, file: File) =>
    new Promise<void>((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      active.current.set(task.id, xhr);
      xhr.open('POST', `/api/identity${uploadBase}/${task.id}/bytes`);
      xhr.setRequestHeader('Content-Type', 'application/octet-stream');
      xhr.setRequestHeader('X-Ayati-Request', 'identity');
      xhr.timeout = 120000;
      xhr.upload.onprogress = (event) => {
        if (event.lengthComputable)
          update(task.id, {
            progress: Math.round((event.loaded / event.total) * 100),
            state: event.loaded === event.total ? 'preparing' : 'uploading',
          });
      };
      xhr.onload = () => {
        active.current.delete(task.id);
        let result;
        try {
          result = JSON.parse(xhr.responseText);
        } catch {
          /* A gateway response may not be JSON. */
        }
        if (xhr.status >= 200 && xhr.status < 300) resolve();
        else
          reject(
            new Error(result?.error ?? 'Upload response was not confirmed. Retry the same file.'),
          );
      };
      xhr.onerror = xhr.ontimeout = () => {
        active.current.delete(task.id);
        reject(new Error('Connection interrupted. Retry or reselect the same file.'));
      };
      xhr.onabort = () => {
        active.current.delete(task.id);
        reject(new Error('Transfer paused. Retry the same file to check its saved status.'));
      };
      xhr.send(file);
    });
  const run = async (task: Upload, file: File) => {
    if (!alive.current || cancelled.current.has(task.id)) return;
    try {
      update(task.id, { state: 'uploading', error: undefined });
      await identityRequest(uploadBase, {
        id: task.id,
        request: task.request,
        title: task.title,
        size: task.size,
        checksum: task.checksum,
      });
      if (!alive.current || cancelled.current.has(task.id)) return;
      await transfer(task, file);
      update(task.id, { state: 'stored', progress: 100 });
      files.current.delete(task.id);
      if (alive.current) setVersion((n) => n + 1);
    } catch (error) {
      if (!cancelled.current.has(task.id))
        update(task.id, { state: 'interrupted', error: errorMessage(error) });
    }
  };
  const choose = async (selected: FileList | File[]) => {
    if (serial.current) {
      setNotice('Wait for the selected files to finish, or retry an interrupted upload.');
      return;
    }
    serial.current = true;
    setNotice('');
    try {
      for (const file of Array.from(selected).slice(0, 10)) {
        if (!alive.current) break;
        if (!file.size || file.size > 25 * 1024 * 1024 || file.name.length > 180) {
          setNotice('Choose files between 1 byte and 25 MiB with names up to 180 characters.');
          continue;
        }
        if (
          tasksRef.current.filter((t) => !['stored', 'cancelled'].includes(t.state)).length >= 20
        ) {
          setNotice('Finish or cancel pending uploads before adding more.');
          break;
        }
        setNotice(`Checking ${file.name}…`);
        const digest = await crypto.subtle.digest('SHA-256', await file.arrayBuffer());
        if (!alive.current) break;
        const checksum = Array.from(new Uint8Array(digest), (b) =>
          b.toString(16).padStart(2, '0'),
        ).join('');
        const previous = resuming.current;
        if (previous && (previous.checksum !== checksum || previous.size !== file.size)) {
          setNotice('That is a different file. Reselect the original file to resume.');
          resuming.current = undefined;
          break;
        }
        const task = previous ?? {
          id: crypto.randomUUID(),
          request: crypto.randomUUID(),
          title: file.name,
          size: file.size,
          checksum,
          state: 'waiting',
          progress: 0,
        };
        resuming.current = undefined;
        if (!previous) {
          tasksRef.current = [...tasksRef.current.filter((t) => t.state !== 'stored'), task];
          update(task.id, {});
        }
        files.current.set(task.id, file);
        setNotice('');
        await run(task, file);
      }
    } catch (error) {
      if (alive.current) setNotice(errorMessage(error));
    } finally {
      serial.current = false;
    }
  };
  const retry = async (task: Upload) => {
    if (serial.current) return;
    cancelled.current.delete(task.id);
    const file = files.current.get(task.id);
    if (file) {
      serial.current = true;
      try {
        await run(task, file);
      } finally {
        serial.current = false;
      }
    } else {
      resuming.current = task;
      picker.current?.click();
    }
  };
  const cancel = async (task: Upload) => {
    cancelled.current.add(task.id);
    active.current.get(task.id)?.abort();
    try {
      const result = await identityRequest<{ state: string }>(`${uploadBase}/${task.id}`, {});
      update(task.id, {
        state: result.state === 'stored' ? 'stored' : 'cancelled',
        progress: result.state === 'stored' ? 100 : task.progress,
        error: undefined,
      });
      files.current.delete(task.id);
      setVersion((n) => n + 1);
    } catch (error) {
      update(task.id, {
        state: 'interrupted',
        error: `Transfer stopped; cancellation was not confirmed. ${errorMessage(error)}`,
      });
    }
  };
  return (
    <Context value={{ choose: (selected) => void choose(selected), version }}>
      {children}
      <input
        ref={picker}
        type="file"
        hidden
        aria-label="Reselect original file"
        onChange={(event) => {
          if (event.target.files?.length) void choose(event.target.files);
          event.target.value = '';
        }}
      />
      {(tasks.some((t) => t.state !== 'cancelled') || notice) && (
        <aside className="library-upload-tray" aria-label="File uploads">
          <header>
            <strong>Uploads</strong>
            <span>New files are private</span>
            <Button
              aria-label="Dismiss completed uploads"
              onClick={() => {
                tasksRef.current = tasksRef.current.filter((t) => t.state !== 'stored');
                setTasks([...tasksRef.current]);
                setNotice('');
              }}
            >
              ×
            </Button>
          </header>
          {notice && <p role="status">{notice}</p>}
          {tasks
            .filter((t) => t.state !== 'cancelled')
            .map((task) => (
              <div className="library-upload-row" key={task.id}>
                <strong>{task.title}</strong>
                <small>
                  {bytesLabel(task.size)} ·{' '}
                  {task.state === 'stored'
                    ? 'Stored'
                    : task.state === 'preparing'
                      ? 'Verifying and preparing…'
                      : task.state === 'interrupted'
                        ? 'Needs attention'
                        : `${task.progress}% uploaded`}
                </small>
                {task.error && <p role="status">{task.error}</p>}
                {!['stored', 'interrupted'].includes(task.state) && (
                  <progress
                    max={100}
                    value={task.progress}
                    aria-label={`Upload progress for ${task.title}`}
                  />
                )}
                {task.state === 'interrupted' && (
                  <Button onClick={() => void retry(task)}>
                    {files.current.has(task.id) ? 'Retry' : 'Reselect file'}
                  </Button>
                )}
                {task.state !== 'stored' && (
                  <Button onClick={() => void cancel(task)}>Cancel</Button>
                )}
              </div>
            ))}
        </aside>
      )}
    </Context>
  );
}
