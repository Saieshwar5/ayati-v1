import { useEffect, useRef, useState } from 'react';
import { errorMessage } from '../requests';
import { loadCatalog, type CatalogApp, type CatalogPage } from './apps-model';

type Shown = { key: string; apps: CatalogApp[]; next: string | null; error: string };
type Paging = { busy: boolean; error: string };
const keyOf = (query: string, category: string) => `${category}\n${query}`;

/** Server-side search and paging for the Add app catalog. Typing waits for a short
 * pause; each new search aborts the one before, and an answer for an older search,
 * category or page is ignored, so it can never replace the current list. */
export function useCatalog(text: string, category: string) {
  const [query, setQuery] = useState(text.trim());
  useEffect(() => {
    const timer = window.setTimeout(() => setQuery(text.trim()), 250);
    return () => window.clearTimeout(timer);
  }, [text]);
  const [shown, setShown] = useState<Shown>({ key: '', apps: [], next: null, error: '' });
  const [categories, setCategories] = useState<CatalogPage['categories']>();
  const [paging, setPaging] = useState<Paging>({ busy: false, error: '' });
  const [attempt, setAttempt] = useState(0);
  const pageRequest = useRef<AbortController>(undefined);
  useEffect(() => {
    const key = keyOf(query, category);
    const controller = new AbortController();
    pageRequest.current?.abort(); pageRequest.current = undefined;
    setPaging({ busy: false, error: '' });
    loadCatalog({ query, category }, controller.signal).then(page => {
      if (controller.signal.aborted) return;
      setShown({ key, apps: page.apps, next: page.next_cursor, error: '' });
      setCategories(page.categories);
    }, failure => {
      if (!controller.signal.aborted) setShown({ key, apps: [], next: null, error: errorMessage(failure) });
    });
    return () => controller.abort();
  }, [query, category, attempt]);
  useEffect(() => () => pageRequest.current?.abort(), []);
  const key = keyOf(query, category);
  const current = shown.key === key;
  /** The next page of the same search; a second click while one loads does nothing. */
  const more = () => {
    if (!current || !shown.next || pageRequest.current) return;
    const controller = new AbortController();
    pageRequest.current = controller;
    setPaging({ busy: true, error: '' });
    loadCatalog({ query, category, cursor: shown.next }, controller.signal).then(page => {
      if (controller.signal.aborted) return;
      setShown(old => old.key !== key ? old
        : { ...old, apps: [...old.apps, ...page.apps.filter(app => !old.apps.some(seen => seen.id === app.id))], next: page.next_cursor });
      setPaging({ busy: false, error: '' });
    }, failure => {
      if (!controller.signal.aborted) setPaging({ busy: false, error: errorMessage(failure) });
    }).finally(() => { if (pageRequest.current === controller) pageRequest.current = undefined; });
  };
  return { query, categories, apps: shown.apps, next: current ? shown.next : null, error: current ? shown.error : '', loading: !current,
    paging, more, reload: () => { setShown(old => ({ ...old, key: '' })); setAttempt(n => n + 1); } };
}
