import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import type { ViewLifetime } from '../../view-lifetime';
import { ResultDialog } from './provider';
import type { ResultSource, Selection } from './model';
/** React owns only this portal. The compatibility transcript keeps its existing owner. */
export function mountLegacyResults(view: ViewLifetime) {
  const host = document.createElement('div');
  view.append(host);
  const root = createRoot(host);
  let selection: Selection | undefined;
  const source: ResultSource = {
    read: (path, signal) => view.api(path, { signal }),
    href: view.apiUrl,
    response: id => `/messages/${encodeURIComponent(id)}/response`,
    file: id => `/files/${encodeURIComponent(id)}`,
  };
  const render = () => { if (view.active) root.render(<StrictMode>{selection && <ResultDialog selection={selection} source={source} close={() => {
    const previous = selection; selection = undefined; render();
    if (previous?.kind === 'response') queueMicrotask(() => { if (view.active) view.querySelectorAll<HTMLButtonElement>('[data-open-response]').find(node => node.dataset.openResponse === previous.id)?.focus(); });
  }} />}</StrictMode>); };
  view.cleanup(() => { selection = undefined; queueMicrotask(() => root.unmount()); });
  return {
    isOpen: () => Boolean(selection),
    openResponse: (id: string) => { selection = { kind: 'response', id }; render(); },
    openFile: (file: { id: string; name: string }) => { selection = { kind: 'file', ...file }; render(); },
  };
}
