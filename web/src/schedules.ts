import './schedules.css';
import { workPanels } from './work-panels';
import { api, post, type Task, type ChatMessage } from './types';
import { element } from './view';
import { installScheduleForm } from './schedule-form';
import type { Profile } from './agents';

type Schedule = { id: string; content: string; run_at_ms: number; interval_seconds: number | null; next_run_ms: number | null; state: string; last_task_id: string | null };
type Run = { due_at_ms: number; task: Task };
type Schedules = { schedules: Schedule[]; available: boolean; dispatch_paused: boolean };

export function installSchedules(details: HTMLDialogElement): (profile: Profile) => void {
  const section = element('section');
  section.innerHTML = '<h3>Schedules</h3><p>Run this agent’s work once or at a fixed interval.</p>';
  const launch = element('button', 'files-button', 'Manage schedules'); launch.disabled = true;
  section.append(launch); details.querySelector('.details-note')!.before(section);
  const dialog = document.createElement('dialog'); dialog.id = 'agent-schedules'; dialog.className = 'agent-setup schedule-dialog';
  dialog.setAttribute('aria-labelledby', 'schedules-title');
  dialog.innerHTML = `<div class="panel-header"><h2 id="schedules-title">Automation</h2><button class="icon-button" aria-label="Close automation">×</button></div>
    <div class="automation-tabs" role="tablist" aria-label="Automation type"><button id="schedules-tab" role="tab" aria-controls="schedules-content" aria-selected="true">Schedules</button><button id="triggers-tab" role="tab" aria-controls="triggers-content" aria-selected="false" tabindex="-1">Triggers</button></div>
    <section id="schedules-content" role="tabpanel" aria-labelledby="schedules-tab">
    <p id="schedules-agent"></p><p>Pausing stops queued occurrences. Active work keeps running; use its Stop control. After downtime, missed intervals coalesce to the latest due occurrence.</p>
    <p>To change a schedule’s instructions or timing, pause it and create another. Existing runs remain available.</p>
    <div class="schedule-toolbar"><button class="files-button" id="schedules-refresh">Refresh</button><p id="schedules-status" role="status"></p></div>
    <p id="schedules-dispatch" hidden>Agent dispatch is paused. Schedules stay saved. Use Start in the agent browser/environment controls or send a new message to resume dispatch.</p>
    <div id="schedules-list"></div><section id="schedule-history" hidden aria-labelledby="schedule-history-title"><h3 id="schedule-history-title">Recent runs</h3><p>Up to 20 occurrences, newest first. Use Refresh to update run statuses. A one-time schedule being “completed” means its work was accepted; inspect the run outcome below.</p><div id="schedule-runs"></div><div id="schedule-result" role="status"></div></section>
    <section id="schedule-create"></section></section>
    <section id="triggers-content" role="tabpanel" aria-labelledby="triggers-tab" hidden><h3>Event triggers</h3><p>Triggers will start this agent’s work when an event occurs, such as a new customer query. Event sources and trigger management are not available yet.</p></section>`;
  const toggle = element('button', 'files-button', 'Automation'); toggle.id = 'automation-button'; toggle.disabled = true;
  document.querySelector('.header')!.append(toggle);
  const work = workPanels.register(dialog, toggle);
  const list = dialog.querySelector<HTMLElement>('#schedules-list')!;
  const status = dialog.querySelector('#schedules-status')!;
  const refreshButton = dialog.querySelector<HTMLButtonElement>('#schedules-refresh')!;
  const history = dialog.querySelector<HTMLElement>('#schedule-history')!;
  const runs = dialog.querySelector('#schedule-runs')!;
  const result = dialog.querySelector('#schedule-result')!;
  let current: Profile | undefined;
  let busy = false;
  let fresh = false;
  let historyId: string | undefined;
  let historyVersion = 0;
  let fingerprint = '';
  let poll: ReturnType<typeof setInterval> | undefined;
  const enableForm = installScheduleForm(dialog.querySelector('#schedule-create')!, refresh);
  const when = (ms: number): string => new Date(ms).toLocaleString();
  function controls(): void {
    refreshButton.disabled = busy;
    list.querySelectorAll<HTMLButtonElement>('button').forEach(button => { button.disabled = busy || !fresh; });
  }
  async function showResult(task: string): Promise<void> {
    const version = ++historyVersion;
    result.textContent = 'Loading result…';
    try {
      const data = await api<{ task: Task; messages: ChatMessage[] }>(`/tasks/${encodeURIComponent(task)}/history`);
      if (version !== historyVersion) return;
      result.replaceChildren(element('h3', '', `Run outcome: ${data.task.status}`), element('p', '', data.task.detail));
      for (const message of data.messages) result.append(element('strong', '', message.role === 'user' ? 'Scheduled request' : current?.name ?? 'Agent'), element('p', 'schedule-message', message.content));
    } catch (error) { if (version === historyVersion) result.textContent = (error as Error).message; }
  }
  async function showRuns(id: string): Promise<void> {
    historyId = id; const version = ++historyVersion;
    history.hidden = false; result.replaceChildren(); runs.textContent = 'Loading runs…';
    try {
      const data = await api<{ runs: Run[] }>(`/schedules/${encodeURIComponent(id)}/runs`);
      if (version !== historyVersion) return;
      runs.replaceChildren();
      if (!data.runs.length) runs.textContent = 'No accepted runs yet.';
      for (const run of data.runs) {
        const row = element('div', 'schedule-run');
        const open = element('button', 'text-button', `${when(run.due_at_ms)} · ${run.task.status}`);
        open.onclick = () => void showResult(run.task.id);
        row.append(open, element('p', '', run.task.detail)); runs.append(row);
      }
    } catch (error) { if (version === historyVersion) runs.textContent = (error as Error).message; }
  }
  function draw(data: Schedules): void {
    const next = JSON.stringify(data.schedules);
    if (next === fingerprint) return;
    fingerprint = next; list.replaceChildren();
    if (!data.schedules.length) list.append(element('p', '', 'No schedules for this agent yet.'));
    for (const schedule of data.schedules) {
      const card = element('article', 'schedule-card'); card.dataset.schedule = schedule.id;
      card.append(element('h3', '', schedule.content), element('p', '', schedule.interval_seconds === null ? 'Run once' : `Every ${schedule.interval_seconds / 60} minutes (fixed interval)`));
      const label = schedule.state === 'completed' ? 'Completed · occurrence accepted' : schedule.state === 'paused' ? 'Paused' : 'Active';
      card.append(element('p', '', `${label} · First run: ${when(schedule.run_at_ms)}`));
      if (schedule.next_run_ms !== null) card.append(element('p', '', `${schedule.state === 'paused' ? 'Saved next due' : 'Next due'}: ${when(schedule.next_run_ms)}`));
      const actions = element('div', 'schedule-toolbar');
      if (schedule.state !== 'completed') {
        const toggle = element('button', 'files-button', schedule.state === 'active' ? 'Pause' : 'Resume');
        toggle.onclick = async () => {
          if (busy || !fresh) return;
          busy = true; controls();
          let failure = '';
          try {
            await post(`/schedules/${schedule.id}/enabled`, { enabled: schedule.state !== 'active' });
          } catch (error) { failure = `${(error as Error).message} Refresh to inspect the saved state before retrying.`; }
          finally { busy = false; fresh = false; controls(); }
          await refresh();
          if (failure) status.textContent = failure;
        };
        actions.append(toggle);
      }
      const inspect = element('button', 'text-button', 'View runs'); inspect.onclick = () => void showRuns(schedule.id);
      actions.append(inspect); card.append(actions); list.append(card);
    }
  }
  async function refresh(): Promise<void> {
    if (busy) return;
    busy = true; controls();
    try {
      const data = await api<Schedules>('/schedules');
      fresh = true; enableForm(data.available);
      dialog.querySelector<HTMLElement>('#schedules-dispatch')!.hidden = !data.dispatch_paused;
      draw(data);
      status.textContent = data.available ? 'Schedules are up to date.' : 'Schedules require the Goose runtime.';
    } catch (error) {
      fresh = false; enableForm(false);
      status.textContent = `Could not refresh: ${(error as Error).message} Controls are disabled until reconnected.`;
    } finally { busy = false; controls(); }
  }
  function selectTab(id: string): void {
    for (const name of ['schedules', 'triggers']) {
      const active = name === id;
      const tab = dialog.querySelector<HTMLButtonElement>(`#${name}-tab`)!;
      tab.setAttribute('aria-selected', String(active)); tab.tabIndex = active ? 0 : -1;
      dialog.querySelector<HTMLElement>(`#${name}-content`)!.hidden = !active;
    }
  }
  const tabs = dialog.querySelectorAll<HTMLButtonElement>('[role=tab]');
  tabs.forEach((tab, index) => {
    tab.onclick = () => selectTab(index === 0 ? 'schedules' : 'triggers');
    tab.onkeydown = event => {
      if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
      event.preventDefault(); const next = event.key === 'Home' ? 0 : event.key === 'End' ? 1 : 1 - index;
      tabs[next].click(); tabs[next].focus();
    };
  });
  launch.onclick = () => { selectTab('schedules'); work.open(); };
  dialog.addEventListener('workpanelopen', () => {
    dialog.querySelector('#schedules-agent')!.textContent = `Schedules for ${current?.name ?? 'this agent'}`;
    void refresh();
    poll = setInterval(() => { if (work.isOpen() && !document.hidden) void refresh(); }, 5000);
  });
  dialog.querySelector<HTMLButtonElement>('[aria-label="Close automation"]')!.onclick = () => work.close();
  dialog.addEventListener('workpanelclose', () => {
    clearInterval(poll); ++historyVersion; historyId = undefined; history.hidden = true;
  });
  refreshButton.onclick = async () => { await refresh(); if (historyId) await showRuns(historyId); };
  return profile => { current = profile; launch.disabled = !profile.editable; toggle.disabled = !profile.editable; };
}
