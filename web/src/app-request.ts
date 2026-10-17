import { api } from './types';
import { element } from './view';

export function installAppRequest(root: HTMLElement): void {
  const container = root.querySelector<HTMLElement>('#app-request')!;
  root.querySelector<HTMLButtonElement>('#ask-app')!.onclick = async () => {
    container.hidden = false; container.textContent = 'Loading agents…';
    try {
      const data = await api<{ agents: { profile: { id: string; name: string; state: string } }[] }>('/agents');
      const form = document.createElement('form'), agent = document.createElement('select'), text = document.createElement('textarea');
      agent.setAttribute('aria-label', 'App building agent'); text.setAttribute('aria-label', 'App requirements'); text.required = true; text.maxLength = 7000;
      for (const { profile } of data.agents.filter(a => a.profile.state === 'ready')) { const option = document.createElement('option'); option.value = profile.id; option.textContent = profile.name; agent.append(option); }
      const send = element('button', 'files-button', 'Send app request'); send.type = 'submit'; const status = element('p'); status.setAttribute('role', 'status');
      form.append(agent, text, element('p', '', 'Describe the work, records and forms you need. This starts an agent task and may incur model charges. You will review a working preview before activation.'), send, status); container.replaceChildren(form);
      let pending: { id: string; agent: string; content: string; request: string } | undefined;
      const key = 'ayati.app.request.pending';
      try { const raw = sessionStorage.getItem(key); if (raw) { const saved = JSON.parse(raw); if (typeof saved.id === 'string' && typeof saved.agent === 'string' && typeof saved.content === 'string' && typeof saved.request === 'string') pending = saved; } } catch { /* Optional storage. */ }
      if (pending) { agent.value = pending.agent; text.value = pending.request; agent.disabled = true; text.disabled = true; send.textContent = 'Retry same app request'; }
      form.onsubmit = async event => {
        event.preventDefault(); if (send.disabled) return;
        if (!pending && new TextEncoder().encode(text.value).length > 8000) { status.textContent = 'Keep requirements within 8,000 bytes.'; return; }
        pending ??= { id: crypto.randomUUID(), agent: agent.value, request: text.value, content: `Build a local internal app for these requirements using app_write. Supply complete functioning HTML/CSS/JavaScript and record collections. Use the scoped ayati.records API for ordinary data operations. Keep source within the supported budget and preserve existing active data/releases. Do not claim Supabase/Vercel deployment.\n\n${text.value}` };
        try { sessionStorage.setItem(key, JSON.stringify(pending)); } catch { /* Receipt stays in memory. */ }
        send.disabled = true; agent.disabled = true; text.disabled = true;
        try {
          await api('/messages', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Ayati-Agent': pending.agent }, body: JSON.stringify({ content: pending.content, request_id: pending.id }) });
          try { sessionStorage.removeItem(key); } catch { /* Optional storage. */ }
          status.textContent = 'Request accepted. Follow the agent’s work and approvals in its conversation.';
          const link = element('a', 'text-button', 'Open conversation'); link.href = `/?agent=${encodeURIComponent(pending.agent)}`; status.append(link);
        } catch (e) { status.textContent = (e as Error).message; send.disabled = false; send.textContent = 'Retry same app request'; }
      };
    } catch (e) { container.textContent = (e as Error).message; }
  };
}
