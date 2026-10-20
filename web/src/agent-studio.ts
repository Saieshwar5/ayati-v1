import { workspaceId } from './workspace-context';
import { currentView } from './view-lifetime';
const paths: Record<string, string> = {
  browser: '<rect x="3" y="4" width="18" height="16" rx="4"/><path d="M3 9h18M7 6.5h.01M10 6.5h.01"/>',
  automation: '<path d="m13 2-9 12h7l-1 8 10-13h-7z"/>',
  files: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6M8 13h8M8 17h5"/>',
  details: '<path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="3"/><circle cx="15" cy="17" r="3"/>',
};
export function installStudioTools(): void {
  const view = currentView();
  for (const [id, label] of [['browser', 'Browser'], ['automation', 'Automation'], ['files', 'Artifacts'], ['details', 'Settings']]) {
    const button = view.querySelector<HTMLButtonElement>(`#${id}-button`)!;
    const count = button.querySelector('#file-count');
    button.innerHTML = `<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths[id]}</svg>`;
    if (count) button.append(count);
    button.setAttribute('aria-label', label); button.title = label;
  }
  view.querySelector('#agent-details h2')!.textContent = 'Agent settings';
  const identity = view.querySelector<HTMLElement>('.identity')!;
  if (!workspaceId) view.querySelector('.composer-area')!.append(identity);
  else {
    const back = view.querySelector<HTMLButtonElement>('#agent-list-toggle')!;
    back.textContent = '‹ Chats';
    back.setAttribute('aria-label', 'Back to conversations');
    const tools = document.createElement('details'); tools.className = 'chat-agent-tools';
    const summary = document.createElement('summary'); summary.textContent = '•••'; summary.setAttribute('aria-label', 'Agent tools menu'); summary.title = 'Agent tools';
    const actions = document.createElement('div');
    for (const id of ['browser', 'files', 'automation', 'details']) actions.append(view.querySelector(`#${id}-button`)!);
    actions.addEventListener('click', event => { if ((event.target as Element).closest('button')) tools.open = false; });
    tools.addEventListener('focusout', event => { if (!tools.contains(event.relatedTarget as Node | null)) tools.open = false; });
    tools.append(summary, actions); view.querySelector('.header')!.append(tools);
  }
}
