import './home.css';
import { api, type Task, type ChatMessage } from './types';
import { element } from './view';

type Work = Task & { request: string };
type Permission = { id: string; task_id: string; title: string; action: unknown };
type Entry = {
  profile: { id: string; name: string; state: string; detail: string };
  attention: null | { active: Work[]; outcomes: Work[]; outcome_count: number; permissions: Permission[] };
};
type Home = { agents: Entry[] };

export function installHome(): void {
  document.title = 'Home · Ayati';
  const app = document.querySelector<HTMLDivElement>('#app')!;
  app.innerHTML = `<header class="header"><div class="identity"><span class="avatar" aria-hidden="true">a</span><strong>Ayati</strong></div>
    <nav aria-label="Workspace"><a href="/?view=home" aria-current="page">Home</a><a href="/?agent=local-agent">Agents</a></nav></header>
    <main class="home"><div class="home-heading"><div><h1>Your workspace</h1><p>See what is happening and what needs your decision.</p></div><button id="home-refresh" class="files-button">Refresh</button></div>
    <p id="home-connection" role="status">Connecting…</p><p id="home-error" role="alert"></p>
    <section aria-labelledby="overview-title"><h2 id="overview-title">Agents at a glance</h2><div id="home-agents" class="home-agents"></div></section>
    <section aria-labelledby="attention-title"><h2 id="attention-title">Needs your attention</h2><div id="home-attention" class="home-grid"></div></section>
    <section aria-labelledby="work-title"><h2 id="work-title">Active and queued work</h2><div id="home-work" class="home-grid"></div></section></main>`;
  const status = document.querySelector<HTMLElement>('#home-connection')!;
  const error = document.querySelector<HTMLElement>('#home-error')!;
  const refreshButton = document.querySelector<HTMLButtonElement>('#home-refresh')!;
  const dialog = document.createElement('dialog');
  dialog.className = 'home-history';
  dialog.setAttribute('aria-label', 'Request history');
  const close = element('button', 'files-button', 'Close');
  const history = element('div');
  dialog.append(close, history); app.append(dialog);
  close.onclick = () => dialog.close();
  let historyVersion = 0;
  dialog.addEventListener('close', () => { historyVersion++; });
  let loading = false, acting = false, refreshAgain = false;
  let freshAt = 0, fingerprint = '';
  function disableActions(disabled: boolean): void {
    app.querySelectorAll<HTMLButtonElement>('[data-home-action]').forEach(button => { button.disabled = disabled; });
  }
  const scoped = <T>(agent: string, path: string, body?: unknown): Promise<T> => api<T>(path, {
    headers: { 'X-Ayati-Agent': agent, 'Content-Type': 'application/json' },
    signal: AbortSignal.timeout(8000),
    ...(body === undefined ? {} : { method: 'POST', body: JSON.stringify(body) }),
  });
  async function refresh(): Promise<void> {
    if (loading || acting) { refreshAgain = true; return; }
    loading = true;
    try {
      const data = await scoped<Home>('local-agent', '/home');
      freshAt = Date.now();
      const next = JSON.stringify(data);
      if (next !== fingerprint) { draw(data); fingerprint = next; }
      disableActions(false);
      status.textContent = 'Connected · updates automatically';
    } catch {
      freshAt = 0; disableActions(true);
      status.textContent = 'Connection lost. Decisions are disabled until the workspace refreshes.';
    } finally {
      loading = false;
      if (refreshAgain) { refreshAgain = false; void refresh(); }
    }
  }
  function action(label: string, agent: string, path: string, body: unknown = {}): HTMLButtonElement {
    const button = element('button', 'text-button', label);
    button.dataset.homeAction = 'true';
    button.onclick = async () => {
      if (acting) return;
      if (loading) { error.textContent = 'Refreshing the workspace. Please try the decision again in a moment.'; return; }
      if (!freshAt || Date.now() - freshAt > 10000) { disableActions(true); void refresh(); return; }
      acting = true; disableActions(true); error.textContent = '';
      try { await scoped(agent, path, body); }
      catch (e) { error.textContent = `${(e as Error).message} The workspace will refresh; the action was not automatically retried.`; }
      finally { acting = false; await refresh(); }
    };
    return button;
  }
  function conversation(entry: Entry): HTMLAnchorElement {
    const link = element('a', 'text-button', 'Open conversation');
    link.href = `/?agent=${encodeURIComponent(entry.profile.id)}`;
    return link;
  }
  function requestHistory(entry: Entry, work: Work): HTMLButtonElement {
    const button = element('button', 'text-button', 'View request');
    button.onclick = async () => {
      const version = ++historyVersion;
      history.replaceChildren(element('p', '', 'Loading request…'));
      dialog.showModal();
      try {
        const data = await scoped<{ task: Task; messages: ChatMessage[] }>(entry.profile.id, `/tasks/${encodeURIComponent(work.id)}/history`);
        if (version !== historyVersion) return;
        history.replaceChildren(element('h2', '', entry.profile.name), element('p', '', `${data.task.status} · ${data.task.detail}`),
          ...data.messages.map(message => {
            const section = element('section');
            section.append(element('h3', '', message.role === 'user' ? 'You' : entry.profile.name), element('p', 'message-content', message.content));
            return section;
          }));
      } catch (e) { if (version === historyVersion) history.textContent = (e as Error).message; }
    };
    return button;
  }
  function card(entry: Entry, work?: Work): HTMLElement {
    const node = element('article', 'home-card');
    node.dataset.agent = entry.profile.id;
    if (work) node.dataset.task = work.id;
    node.append(element('p', 'home-agent-name', entry.profile.name));
    if (work) {
      node.append(element('p', 'home-request', work.request || 'Request'),
        element('p', 'home-detail', `${work.status} · ${work.detail || 'Waiting to run'}`), requestHistory(entry, work));
    }
    return node;
  }
  function draw(data: Home): void {
    const overview: HTMLElement[] = [], attention: HTMLElement[] = [], work: HTMLElement[] = [];
    for (const entry of data.agents) {
      const { profile, attention: state } = entry;
      const summary = card(entry);
      summary.append(element('p', '', state
        ? `${state.active.length} active or queued · ${state.permissions.length} approvals · ${state.outcome_count} to review`
        : profile.detail));
      if (profile.state === 'ready') summary.append(conversation(entry));
      else {
        const link = element('a', 'text-button', 'Manage agent setup'); link.href = '/?agent=local-agent'; summary.append(link);
      }
      overview.push(summary);
      if (!state) continue;
      for (const permission of state.permissions) {
        const node = card(entry, state.active.find(task => task.id === permission.task_id));
        node.dataset.permission = permission.id;
        node.append(element('h3', '', permission.title), element('pre', 'permission-details', JSON.stringify(permission.action, null, 2)));
        const controls = element('div', 'home-controls');
        controls.append(action('Allow once', profile.id, `/permissions/${encodeURIComponent(permission.id)}`, { allow: true }),
          action('Deny', profile.id, `/permissions/${encodeURIComponent(permission.id)}`, { allow: false }),
          action('Stop request', profile.id, `/tasks/${encodeURIComponent(permission.task_id)}/stop`), conversation(entry));
        node.append(controls); attention.push(node);
      }
      for (const outcome of state.outcomes) {
        const node = card(entry, outcome);
        const controls = element('div', 'home-controls');
        controls.append(action('Mark reviewed', profile.id, `/tasks/${encodeURIComponent(outcome.id)}/review`), conversation(entry));
        node.append(controls); attention.push(node);
      }
      if (state.outcome_count > state.outcomes.length) attention.push(element('p', 'home-detail',
        `${profile.name}: showing ${state.outcomes.length} of ${state.outcome_count} outcomes. Review these to see the next ones.`));
      for (const task of state.active) {
        const node = card(entry, task);
        const controls = element('div', 'home-controls');
        controls.append(action(task.status === 'queued' ? 'Cancel request' : 'Stop request', profile.id, `/tasks/${encodeURIComponent(task.id)}/stop`), conversation(entry));
        node.append(controls); work.push(node);
      }
    }
    document.querySelector('#home-agents')!.replaceChildren(...overview);
    document.querySelector('#home-attention')!.replaceChildren(...(attention.length ? attention : [element('p', 'home-empty', 'You are all caught up.')]));
    document.querySelector('#home-work')!.replaceChildren(...(work.length ? work : [element('p', 'home-empty', 'No work is running or queued. Open an agent to start a conversation.')]));
  }
  refreshButton.onclick = () => { void refresh(); };
  window.addEventListener('focus', () => { freshAt = 0; disableActions(true); void refresh(); });
  document.addEventListener('visibilitychange', () => {
    freshAt = 0; disableActions(true);
    if (!document.hidden) void refresh();
  });
  window.addEventListener('offline', () => { freshAt = 0; disableActions(true); status.textContent = 'Offline. Waiting to reconnect…'; });
  window.addEventListener('online', () => { void refresh(); });
  window.setInterval(() => { if (!document.hidden) void refresh(); }, 2500);
  void refresh();
}
