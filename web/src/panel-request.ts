import { api } from './types';
import { element } from './view';

export function installPanelRequest(container: HTMLElement): void {
  const request = container.querySelector<HTMLElement>('[data-panel-request]')!;
  container.querySelector<HTMLButtonElement>('[data-request-panel]')!.onclick = async () => {
    request.hidden = false; request.replaceChildren(element('p', '', 'Loading agents…'));
    try {
      const data = await api<{ agents: { profile: { id: string; name: string; state: string } }[] }>('/agents');
      const form = document.createElement('form');
      const select = document.createElement('select'); select.setAttribute('aria-label', 'Maintaining agent');
      for (const { profile } of data.agents.filter(a => a.profile.state === 'ready')) { const option = document.createElement('option'); option.value = profile.id; option.textContent = profile.name; select.append(option); }
      const text = document.createElement('textarea'); text.required = true; text.maxLength = 7000; text.setAttribute('aria-label', 'Panel request'); text.placeholder = 'What should this panel show, and how should it stay updated?';
      const submit = element('button', 'files-button', 'Send panel request'); submit.type = 'submit';
      const result = element('p'); result.setAttribute('role', 'status'); form.append(select, text, element('p', '', 'This sends work to the selected agent and may incur model charges. New sources and consequential actions retain their permissions.'), submit, result); request.replaceChildren(form);
      let receipt: { id: string; agent: string; content: string; request: string } | undefined;
      const pendingKey = 'ayati.panel.request.pending';
      try {
        const raw = sessionStorage.getItem(pendingKey);
        if (raw) { const saved = JSON.parse(raw); if (typeof saved.id === 'string' && typeof saved.agent === 'string' && typeof saved.content === 'string' && typeof saved.request === 'string') receipt = saved; }
      } catch { /* Optional tab storage. */ }
      if (receipt) { select.value = receipt.agent; text.value = receipt.request; select.disabled = true; text.disabled = true; submit.textContent = 'Retry same request'; }
      form.onsubmit = async event => {
        event.preventDefault(); if (submit.disabled) return;
        if (!receipt && new TextEncoder().encode(text.value).length > 8000) { result.textContent = 'Keep the panel request within 8,000 bytes.'; return; }
        receipt ??= { id: crypto.randomUUID(), agent: select.value, request: text.value, content: `Create a custom Home panel for this request using panel_write. Use actual authorized sources and label estimates or findings clearly. Propose any update schedule through schedule_create.\n\n${text.value}` };
        try { sessionStorage.setItem(pendingKey, JSON.stringify(receipt)); } catch { /* Keep receipt in memory. */ }
        submit.disabled = true; select.disabled = true; text.disabled = true;
        try {
          await api('/messages', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Ayati-Agent': receipt.agent }, body: JSON.stringify({ content: receipt.content, request_id: receipt.id }) });
          try { sessionStorage.removeItem(pendingKey); } catch { /* Optional storage. */ }
          result.textContent = 'Request accepted. Open the agent’s conversation to follow its work and approvals.';
          const link = element('a', 'text-button', 'Open conversation'); link.href = `/?agent=${encodeURIComponent(receipt.agent)}`; result.append(link);
        } catch (e) { result.textContent = (e as Error).message; submit.disabled = false; submit.textContent = 'Retry same request'; }
      };
    } catch (e) { request.textContent = (e as Error).message; }
  };
}
