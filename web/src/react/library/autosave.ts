import { useEffect, useRef, useState } from 'react';
import { draftKey, request, type Document, type Item } from './model';
import { errorMessage } from '../requests';
type Draft = { title: string; document: Document; revision: number; at: number };
export function usePageSave(
  item: Item,
  onSaved?: (saved: Pick<Item, 'title' | 'document' | 'revision'>) => void,
  enabled = true,
) {
  const draft = useRef<Draft>({
    title: item.title,
    document: item.document,
    revision: item.revision,
    at: Date.now(),
  });
  const acknowledged = useRef(JSON.stringify([item.title, item.document]));
  const attempt = useRef<{ value: string; request: string }>(undefined);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const pending = useRef(false),
    alive = useRef(false),
    blocked = useRef(false);
  const allowed = useRef(item.editable && enabled);
  allowed.current = item.editable && enabled;
  const [status, setStatus] = useState('All changes saved');
  const [dirty, setDirty] = useState(false),
    [busy, setBusy] = useState(false);
  const [recovery, setRecovery] = useState<Draft>();
  const [revision, setRevision] = useState(item.revision);
  const key = draftKey(item.id);
  useEffect(() => {
    if (!enabled) return;
    alive.current = true;
    try {
      const raw = localStorage.getItem(key);
      if (raw && raw.length < 300000) {
        const saved = JSON.parse(raw);
        if (
          typeof saved.title === 'string' &&
          saved.document?.type === 'doc' &&
          JSON.stringify([saved.title, saved.document]) !== acknowledged.current
        )
          setRecovery(saved);
        else localStorage.removeItem(key);
      }
    } catch {
      /* The server copy remains authoritative. */
    }
    const warn = (event: BeforeUnloadEvent) => {
      if (acknowledged.current !== JSON.stringify([draft.current.title, draft.current.document]))
        event.preventDefault();
    };
    window.addEventListener('beforeunload', warn);
    return () => {
      alive.current = false;
      clearTimeout(timer.current);
      window.removeEventListener('beforeunload', warn);
    };
  }, [key, enabled]);
  const save = async () => {
    clearTimeout(timer.current);
    if (pending.current || blocked.current || !allowed.current) return;
    const snapshot = { ...draft.current };
    const value = JSON.stringify([snapshot.title, snapshot.document]);
    if (value === acknowledged.current) return;
    if (!snapshot.title.trim()) {
      setStatus('Add a page title before saving.');
      return;
    }
    pending.current = true;
    setBusy(true);
    setStatus('Saving…');
    const payloadKey = JSON.stringify([snapshot.revision, value]);
    if (attempt.current?.value !== payloadKey)
      attempt.current = { value: payloadKey, request: crypto.randomUUID() };
    try {
      const saved = await request<Item>('', {
        id: item.id,
        request: attempt.current.request,
        revision: snapshot.revision,
        title: snapshot.title,
        document: snapshot.document,
      });
      acknowledged.current = value;
      draft.current.revision = saved.revision;
      if (!alive.current) return;
      setRevision(saved.revision);
      onSaved?.({ title: snapshot.title, document: snapshot.document, revision: saved.revision });
      const changed = value !== JSON.stringify([draft.current.title, draft.current.document]);
      setDirty(changed);
      setStatus(changed ? 'Unsaved changes' : 'All changes saved');
      try {
        if (!changed) localStorage.removeItem(key);
        else localStorage.setItem(key, JSON.stringify(draft.current));
      } catch {
        /* Optional local recovery. */
      }
      if (changed) timer.current = setTimeout(() => void save(), 800);
    } catch (error) {
      if (!alive.current) return;
      const status =
        error && typeof error === 'object' && 'status' in error ? Number(error.status) : 0;
      if ([401, 403, 404, 409].includes(status)) blocked.current = true;
      setStatus(errorMessage(error));
    } finally {
      pending.current = false;
      if (alive.current) setBusy(false);
    }
  };
  const change = (title: string, document: Document) => {
    draft.current = { title, document, revision: draft.current.revision, at: Date.now() };
    const changed = JSON.stringify([title, document]) !== acknowledged.current;
    setDirty(changed);
    if (!blocked.current) setStatus(changed ? 'Unsaved changes' : 'All changes saved');
    try {
      localStorage.setItem(key, JSON.stringify(draft.current));
    } catch {
      setStatus('Local recovery is unavailable. Keep this page open until it is saved.');
    }
    clearTimeout(timer.current);
    if (changed && !blocked.current) timer.current = setTimeout(() => void save(), 1000);
  };
  return {
    change,
    save,
    status,
    busy,
    dirty,
    recovery,
    revision,
    blocked: blocked.current,
    dismissRecovery: () => {
      setRecovery(undefined);
      try {
        localStorage.removeItem(key);
      } catch {
        /* Optional. */
      }
    },
    recovered: () => setRecovery(undefined),
  };
}
