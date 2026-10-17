import { chooseAgent } from './agent-request';
export function installPanelRequest(container: HTMLElement): void {
  container.querySelector<HTMLButtonElement>('[data-request-panel]')!.onclick = () => chooseAgent('panel');
}
