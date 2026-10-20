import { currentView } from './view-lifetime';
export interface WorkPanel {
  open(): void;
  close(focus?: boolean): void;
  toggle(): void;
  isOpen(): boolean;
}

// One contextual work surface. Opening/closing never changes runtime authority.
class WorkPanels {
  private active?: WorkPanel;
  private panels = new Map<string, WorkPanel>();
  private mobile = matchMedia('(max-width: 1000px)');

  register(panel: HTMLDialogElement, launcher: HTMLButtonElement, options: { open?: () => void; close?: () => void; expand?: boolean } = {}): WorkPanel {
    const view = currentView();
    panel.classList.add('work-panel'); panel.hidden = true;
    view.querySelector('.main')!.append(panel);
    launcher.setAttribute('aria-controls', panel.id);
    launcher.setAttribute('aria-expanded', 'false');
    const main = view.querySelector<HTMLElement>('.main')!;
    const owner = document.createElement('p'); owner.className = 'work-panel-owner';
    panel.querySelector('.panel-header')!.after(owner);
    let shown = false;
    const focusLauncher = (): void => {
      (launcher.getClientRects().length ? launcher : view.querySelector<HTMLButtonElement>('#details-button'))?.focus();
    };
    const show = (): void => { if (this.mobile.matches) panel.showModal(); else panel.show(); };
    const close = (focus = true): void => {
      if (!shown) return;
      shown = false;
      if (this.active === control) this.active = undefined;
      main.classList.remove('work-expanded'); delete main.dataset.workPanel;
      panel.close(); panel.hidden = true; launcher.setAttribute('aria-expanded', 'false');
      options.close?.(); panel.dispatchEvent(new Event('workpanelclose'));
      if (focus) focusLauncher();
    };
    const control: WorkPanel = {
      isOpen: () => shown,
      open: () => {
        if (shown) return;
        this.active?.close(false);
        this.active = control; shown = true; panel.hidden = false;
        main.dataset.workPanel = panel.id; launcher.setAttribute('aria-expanded', 'true');
        owner.textContent = view.querySelector('.identity strong')?.textContent ?? 'Selected agent';
        show(); options.open?.(); panel.dispatchEvent(new Event('workpanelopen'));
        panel.querySelector<HTMLElement>('button, input, textarea')?.focus();
      }, close,
      toggle: () => { if (shown) close(); else control.open(); },
    };
    launcher.onclick = control.toggle;
    panel.addEventListener('cancel', e => { e.preventDefault(); close(); });
    panel.addEventListener('close', () => { if (!panel.open) close(); });
    panel.addEventListener('keydown', e => {
      // Native modal dialogs handle their own Escape; do not close parent panels
      // when a nested editor or browser input owns this key.
      if (e.key === 'Escape' && !this.mobile.matches && !e.defaultPrevented) { e.preventDefault(); close(); }
    });
    view.onMedia(this.mobile, () => {
      if (!shown) return;
      const focused = document.activeElement as HTMLElement | null;
      panel.close(); show();
      if (focused && panel.contains(focused)) focused.focus();
    });
    if (options.expand) {
      const expand = document.createElement('button'); expand.className = 'files-button'; expand.id = `${panel.id}-expand`;
      expand.textContent = 'Expand'; expand.setAttribute('aria-pressed', 'false');
      panel.querySelector('.panel-header')!.append(expand);
      const reset = (): void => { expand.textContent = 'Expand'; expand.setAttribute('aria-pressed', 'false'); };
      expand.onclick = () => {
        const expanded = main.classList.toggle('work-expanded');
        expand.textContent = expanded ? 'Restore chat' : 'Expand'; expand.setAttribute('aria-pressed', String(expanded));
      };
      panel.addEventListener('workpanelclose', reset);
    }
    this.panels.set(panel.id, control);
    view.cleanup(() => {
      close(false);
      if (this.panels.get(panel.id) === control) this.panels.delete(panel.id);
    });
    return control;
  }

  close(id: string, focus = false): void { this.panels.get(id)?.close(focus); }
}
export const workPanels = new WorkPanels();
