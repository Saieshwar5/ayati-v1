import { element } from './view';
import './agent-request.css';

export function installConversationIntent(input: HTMLTextAreaElement): void {
  const url = new URL(location.href), intent = url.searchParams.get('intent');
  const item = url.searchParams.get('item');
  const reference = item && /^[a-zA-Z0-9-]{1,80}$/.test(item) ? item : '';
  const starters: Record<string, string> = {
    panel: 'Create a panel on my Home that shows ',
    app: 'Build an app that helps me ',
    'edit-panel': `Help me change my Home panel ${reference}. I would like to `,
    'edit-app': `Help me improve my app ${reference}. I would like to `,
  };
  if (!intent || !(intent in starters)) return;
  const topic = url.searchParams.get('topic');
  const starter = intent === 'panel' && topic && ['orders', 'stock', 'appointments'].includes(topic) ? `Create a panel on my Home that helps me keep track of ${topic}. ` : starters[intent];
  const banner = element('div', 'conversation-intent');
  banner.append(element('p', '', input.value ? 'Your earlier draft is saved. Add this request when you’re ready.' : 'Describe what you need, then send it to this agent.'));
  const use = element('button', 'text-button', input.value ? 'Add to draft' : 'Start request');
  const dismiss = element('button', 'text-button', 'Dismiss');
  const clear = (): void => { url.searchParams.delete('intent'); url.searchParams.delete('item'); url.searchParams.delete('topic'); history.replaceState(null, '', url); banner.remove(); };
  use.onclick = () => { input.value += (input.value ? '\n\n' : '') + starter; input.dispatchEvent(new Event('input')); input.focus(); clear(); };
  dismiss.onclick = clear; banner.append(use, dismiss);
  document.querySelector('.composer-area')!.prepend(banner);
  if (!input.value) use.click();
}
