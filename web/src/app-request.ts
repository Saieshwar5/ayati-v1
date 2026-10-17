import { chooseAgent } from './agent-request';
export function installAppRequest(root: HTMLElement): void {
  root.querySelector<HTMLButtonElement>('#ask-app')!.onclick = () => chooseAgent('app');
}
