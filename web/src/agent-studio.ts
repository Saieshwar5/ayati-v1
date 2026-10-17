const paths: Record<string, string> = {
  browser: '<rect x="3" y="4" width="18" height="16" rx="4"/><path d="M3 9h18M7 6.5h.01M10 6.5h.01"/>',
  automation: '<path d="m13 2-9 12h7l-1 8 10-13h-7z"/>',
  files: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8zM14 2v6h6M8 13h8M8 17h5"/>',
  details: '<path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="3"/><circle cx="15" cy="17" r="3"/>',
};
export function installStudioTools(): void {
  for (const [id, label] of [['browser', 'Browser'], ['automation', 'Automation'], ['files', 'Artifacts'], ['details', 'Settings']]) {
    const button = document.querySelector<HTMLButtonElement>(`#${id}-button`)!;
    const count = button.querySelector('#file-count');
    button.innerHTML = `<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths[id]}</svg>`;
    if (count) button.append(count);
    button.setAttribute('aria-label', label); button.title = label;
  }
  document.querySelector('#agent-details h2')!.textContent = 'Agent settings';
  const identity = document.querySelector<HTMLElement>('.identity')!;
  document.querySelector('.composer-area')!.append(identity);
}
