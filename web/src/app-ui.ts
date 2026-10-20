import { element } from './view.ts';

export function uiButton(label: string, action: () => void, primary = false): HTMLButtonElement {
  const button = element('button', `ui-button${primary ? ' primary' : ''}`, label);
  button.type = 'button'; button.onclick = action;
  return button;
}

export function searchInput(label: string): HTMLInputElement {
  const input = element('input', 'ui-search');
  input.type = 'search'; input.placeholder = label; input.setAttribute('aria-label', label);
  return input;
}

export function createDialog(title: string, drawer = false) {
  const dialog = element('dialog', `ui-dialog${drawer ? ' ui-drawer' : ''}`);
  const heading = element('h2', '', title); heading.id = `dialog-${crypto.randomUUID()}`;
  dialog.setAttribute('aria-labelledby', heading.id);
  const close = uiButton('×', () => dialog.close()); close.classList.add('ui-icon');
  close.setAttribute('aria-label', 'Close');
  const header = element('header', 'ui-dialog-header'); header.append(heading, close);
  const body = element('div', 'ui-dialog-body');
  const footer = element('footer', 'ui-dialog-footer');
  const status = element('p', 'ui-status'); status.setAttribute('role', 'status');
  footer.append(status); dialog.append(header, body, footer); document.body.append(dialog);
  let pending = 0;
  let enabled: (HTMLButtonElement | HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement)[] = [];
  dialog.addEventListener('cancel', event => { if (pending) event.preventDefault(); });
  dialog.addEventListener('close', () => dialog.remove());
  return {
    dialog, body, footer, status,
    setBusy(busy: boolean) {
      if (busy && pending === 0) {
        enabled = [...dialog.querySelectorAll<HTMLButtonElement | HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement>('button,input,select,textarea')].filter(control => !control.disabled);
        enabled.forEach(control => { control.disabled = true; });
      }
      pending = Math.max(0, pending + (busy ? 1 : -1)); close.disabled = pending > 0;
      if (!pending) { enabled.forEach(control => { control.disabled = false; }); enabled = []; }
      dialog.setAttribute('aria-busy', String(pending > 0));
    },
    show() { dialog.showModal(); },
  };
}

/** Tabs retain their fields and scroll positions while switching sections. */
export function installTabs(root: HTMLElement, sections: { title: string; content: HTMLElement }[]) {
  const list = element('div', 'ui-tabs'); list.setAttribute('role', 'tablist');
  const id = `tabs-${crypto.randomUUID()}`;
  const buttons = sections.map(({ title, content }, index) => {
    const button = uiButton(title, () => select(index));
    button.id = `${id}-${index}`; button.setAttribute('role', 'tab');
    content.id = `${id}-panel-${index}`; content.setAttribute('role', 'tabpanel');
    content.setAttribute('aria-labelledby', button.id); content.tabIndex = 0;
    button.setAttribute('aria-controls', content.id);
    button.onkeydown = event => {
      const target = event.key === 'ArrowRight' ? (index + 1) % sections.length
        : event.key === 'ArrowLeft' ? (index + sections.length - 1) % sections.length
        : event.key === 'Home' ? 0 : event.key === 'End' ? sections.length - 1 : -1;
      if (target < 0) return;
      event.preventDefault(); select(target); buttons[target].focus();
    };
    list.append(button); return button;
  });
  function select(index: number): void {
    sections.forEach(({ content }, i) => {
      content.hidden = i !== index;
      buttons[i].setAttribute('aria-selected', String(i === index));
      buttons[i].tabIndex = i === index ? 0 : -1;
    });
  }
  root.append(list, ...sections.map(s => s.content)); select(0);
  return select;
}
