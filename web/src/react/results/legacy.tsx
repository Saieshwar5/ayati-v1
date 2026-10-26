import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import type { ViewLifetime } from '../../view-lifetime';
import { ResultDialog } from './provider';
import { InlineResponse } from './inline-response';
import type { ResultSource, Selection } from './model';
/** React owns only this portal. The compatibility transcript keeps its existing owner. */
export function mountLegacyResults(view: ViewLifetime) {
  const host = document.createElement('div');
  view.append(host);
  const root = createRoot(host);
  let selection: Selection | undefined;
  const responses = new Map<string, { host: HTMLElement; root: ReturnType<typeof createRoot> }>();
  const expanded = new Set<string>();
  const reading = (key: string, open: boolean) => { if (open) expanded.add(key); else expanded.delete(key); };
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
  const retainResponses = (ids: string[]) => {
    const keep = new Set(ids);
    for (const [id, response] of responses) if (!keep.has(id)) {
      responses.delete(id); queueMicrotask(() => response.root.unmount());
    }
  };
  view.cleanup(() => { selection = undefined; retainResponses([]); queueMicrotask(() => root.unmount()); });
  return {
    isOpen: () => Boolean(selection) || expanded.size > 0,
    retainResponses,
    response: (id: string, text: string) => {
      let response = responses.get(id);
      if (!response) {
        const host = document.createElement('div'); host.className = 'private-response';
        response = { host, root: createRoot(host) }; responses.set(id, response);
      }
      response.root.render(<StrictMode><InlineResponse id={id} text={text} source={source} reading={reading}
        openReader={() => { selection = { kind: 'response', id }; render(); }} /></StrictMode>);
      return response.host;
    },
    openResponse: (id: string) => { selection = { kind: 'response', id }; render(); },
    openFile: (file: { id: string; name: string }) => { selection = { kind: 'file', ...file }; render(); },
  };
}
