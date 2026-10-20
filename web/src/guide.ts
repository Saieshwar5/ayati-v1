import { workspaceUrl } from './workspace-context';
import { element } from './view';
import './agent-request.css';

// Reuse the real conversation: drafts, streaming, approvals and Stop have one implementation.
export function openGuide(): void {
  const existing = document.querySelector<HTMLDialogElement>('.guide-drawer');
  if (existing) { existing.showModal(); return; }
  const dialog = document.createElement('dialog'); dialog.className = 'guide-drawer'; dialog.setAttribute('aria-label', 'Ask Relay');
  const header = element('header'); const full = element('a', 'text-button', 'Open conversation ↗'); full.href = workspaceUrl('/?agent=local-agent');
  const close = element('button', 'icon-button', '×'); close.setAttribute('aria-label', 'Close Relay');
  header.append(element('span', '', 'Your guide · Always here'), full, close);
  const frame = document.createElement('iframe'); frame.title = 'Conversation with Relay'; frame.src = workspaceUrl('/?agent=local-agent&guide=1');
  dialog.append(header, frame); document.body.append(dialog); dialog.showModal(); close.onclick = () => dialog.close();
  // Keep the frame on close so an unsent draft or open approval remains intact.
}
