// Work: one flat row per agent with a side panel, and the open tasks.
(() => {
const P = window.Proto;
const rank = ['fix', 'wait', 'work', 'idle', 'off'];
const labels = { fix: ['Needs fixing', 'bad'], wait: ['Waiting on you', 'wait'], work: ['Working', 'work'], idle: ['Ready', 'done'], off: ['Paused', ''] };
/** An agent's state follows the records: what waits on you, then what it is doing. */
const stateOf = agent => {
  if (P.s.allPaused || agent.paused) return 'off';
  const needs = P.needsFor(agent.name);
  return needs.some(item => item.group === 'fix') ? 'fix' : needs.length ? 'wait' : agent.doing.length ? 'work' : 'idle';
};
const nowOf = agent => P.s.allPaused ? 'All agents are paused' : agent.paused ? agent.paused
  : agent.doing.length ? agent.doing[0].title + (agent.doing.length > 1 ? ` · and ${agent.doing.length - 1} more` : '') : 'Nothing running';
const inScope = () => P.s.agents.filter(agent => P.s.scope === 'all' || !agent.owner);
const scopePick = () => `<span class="seg" role="group" aria-label="Whose agents">${[['mine', 'My agents'], ['all', 'All agents']].map(([id, label]) =>
  `<button type="button" data-act="scope" data-scope="${id}" aria-pressed="${P.s.scope === id}">${label}</button>`).join('')}</span>`;
P.scopePick = scopePick;

function agentsView() {
  const w = P.s.work, all = inScope().filter(agent => P.matches(agent.name, agent.role));
  const sorted = [...all].sort((a, b) => rank.indexOf(stateOf(a)) - rank.indexOf(stateOf(b)));
  const list = sorted.filter(agent => w.filter === 'all' || stateOf(agent) === w.filter);
  if (!list.some(agent => agent.name === w.agent)) w.agent = list[0]?.name;
  const agent = list.find(entry => entry.name === w.agent);
  const pills = [['all', `All ${all.length}`], ...rank.map(id => [id, `${labels[id][0]} ${all.filter(entry => stateOf(entry) === id).length}`])];
  const rows = list.map(entry => {
    const state = stateOf(entry), needs = P.needsFor(entry.name).length;
    return `<button type="button" class="arow agrid" data-agent="${entry.name}" aria-current="${entry === agent}">
      <span class="who">${P.face(entry.name, 'lg')}<span><strong>${entry.name}</strong><small>${entry.role}${entry.owner ? ` · ${entry.owner}’s` : ''}</small></span></span>
      <span class="doing"><i class="dot ${state}"></i><span>${nowOf(entry)}</span></span>
      <span>${needs ? `<span class="count">${needs}</span>` : '<small>—</small>'}</span>
      <small class="hide-s">${entry.next}</small><small class="hide-s">${entry.today}</small></button>`;
  }).join('');
  return `<div class="split wide${P.s.paneOpen && agent ? ' open' : ''}"><div class="list">
      <div class="pills">${pills.map(([id, label]) => `<button type="button" class="pill" data-act="filter" data-filter="${id}" aria-pressed="${w.filter === id}">${id === 'all' ? '' : `<i class="dot ${id}"></i>`}${label}</button>`).join('')}</div>
      <div class="agrid cols"><span>Agent</span><span>Right now</span><span>Needs you</span><span class="hide-s">Next run</span><span class="hide-s">Today</span></div>
      ${rows || P.empty('No agents to show', P.s.scope === 'mine' ? 'Choose All agents to see the rest.' : '')}
    </div><section class="pane" aria-label="Agent">${agent ? agentPane(agent) : ''}</section></div>`;
}
function agentPane(agent) {
  const state = stateOf(agent), needs = P.needsFor(agent.name);
  const section = (title, rows) => rows.length ? `<h3>${title}</h3>${rows.join('')}` : '';
  return `<div class="pane-scroll"><article class="pane-read">
    <button type="button" class="back" data-act="back">‹ Agents</button>
    <header class="ahead">${P.face(agent.name, 'xl')}<div><h2>${agent.name}</h2><small>${agent.role} · managed by ${agent.owner ?? 'you'}</small></div>${P.chip([labels[state][1], labels[state][0]])}</header>
    <div class="btns">${P.s.allPaused ? '' : P.button(agent.paused ? 'Resume' : 'Pause', 'pause')}${P.button(`Set up ${agent.name} →`, 'setup')}${P.button('Message', 'message')}</div>
    ${section(`Needs you · ${needs.length}`, needs.map(item => `<button type="button" class="line go" data-act="open-item" data-item="${item.id}"><span><strong>${P.titleOf(item)}</strong><small>${item.urgent ?? item.age}</small></span><span class="link">Open ›</span></button>`))}
    ${section('Doing now', agent.doing.map(job => `<div class="line"><span><strong>${job.title}</strong><small>${job.meta}</small></span>${job.stop ? P.button('Stop', 'stop', { data: `data-job="${job.id}"` }) : ''}</div>`))}
    ${section('Coming up', agent.coming.map(([time, title, meta]) => `<div class="line"><time>${time}</time><span><strong>${title}</strong><small>${meta}</small></span></div>`))}
    <h3>Earlier today</h3><div class="line"><span><small>${agent.today}</small></span>${P.button('All history', 'agent-history')}</div>
    <p class="note plain"><b>Last 30 days.</b> ${agent.month}</p>
  </article></div>`;
}

const order = ['Needs fixing', 'Late', 'Waiting', 'Working', 'Open', 'Follow-up set'];
const tasksInScope = () => P.s.tasks.filter(task => P.matches(task.title, task.agent));
function tasksView() {
  const w = P.s.work, list = tasksInScope(), task = list.find(entry => entry.id === w.task);
  const groups = order.map(status => [status, list.filter(entry => entry.status === status)]).filter(([, rows]) => rows.length);
  const pane = task ? P.pane({ chip: [task.tone, task.status], title: task.title, agent: task.agent, facts: task.meta,
      notes: task.item && P.s.items.some(item => item.id === task.item && !item.done) ? [['wait', 'Part of this task is waiting in Needs you.']] : [],
      steps: [['ok', 'Promised by the agent'], [task.tone === 'bad' ? 'bad' : 'wait', task.status]] },
    { back: 'Tasks', bar: (task.item && P.s.items.some(item => item.id === task.item && !item.done) ? P.button('Open in Needs you', 'open-item', { primary: true, data: `data-item="${task.item}"` }) : '')
      + P.button('Mark done', 'task-close', { data: 'data-how="Marked done."' }) + P.button('Cancel task', 'task-close', { data: 'data-how="Task cancelled."' }) + P.button('Not a task', 'task-close', { data: 'data-how="Marked as not a task."' }) })
    : `<div class="pane-scroll">${P.empty('Choose a task', 'Its steps and how to close it show here.')}</div>`;
  return `<div class="split wide${P.s.paneOpen && task ? ' open' : ''}"><div class="list">${groups.map(([status, rows]) => `<h3 class="group">${status} · ${rows.length}</h3>${rows.map(entry =>
      `<button type="button" class="row" data-task="${entry.id}" aria-current="${entry === task}"><span>${P.face(entry.agent)}</span>
        <span class="row-main"><span class="row-title">${entry.title}</span><span class="row-meta"><span>${entry.agent} · ${entry.meta}</span></span></span>${P.chip([entry.tone, entry.status])}</button>`).join('')}`).join('')
      || P.empty('Nothing open', 'Promises agents make appear here until they are done.')}</div>
    <section class="pane" aria-label="Task">${pane}</section></div>`;
}

P.views.work = {
  search: 'Search agents and tasks',
  render(root) {
    const w = P.s.work;
    root.innerHTML = `<div class="stack"><div class="toolbar"><span class="seg" role="group" aria-label="Show">
        <button type="button" data-act="mode" data-mode="agents" aria-pressed="${w.mode === 'agents'}">Agents</button>
        <button type="button" data-act="mode" data-mode="tasks" aria-pressed="${w.mode === 'tasks'}">Tasks ${P.s.tasks.length}</button></span>
      <span class="grow"></span>${scopePick()}
      <details class="menu"><summary aria-label="More">⋯</summary><div><button type="button" data-act="pause-all">${P.s.allPaused ? 'Resume agents' : 'Pause all agents…'}</button></div></details></div>
      ${w.mode === 'agents' ? agentsView() : tasksView()}</div>`;
  },
  click(target) {
    const w = P.s.work, row = target.closest('[data-agent], [data-task]');
    const button = target.closest('[data-act]');
    if (!button && row) { if (row.dataset.agent) w.agent = row.dataset.agent; else w.task = row.dataset.task; P.s.paneOpen = true; return P.draw(); }
    if (!button) return;
    const act = button.dataset.act, agent = P.s.agents.find(entry => entry.name === w.agent);
    if (act === 'mode') { w.mode = button.dataset.mode; P.s.paneOpen = false; }
    else if (act === 'scope') P.s.scope = button.dataset.scope;
    else if (act === 'filter') w.filter = button.dataset.filter;
    else if (act === 'back') P.s.paneOpen = false;
    else if (act === 'open-item') { P.selectItem(button.dataset.item); P.s.paneOpen = true; return P.go('needs'); }
    else if (act === 'pause-all') return P.s.allPaused ? P.resumeAll() : document.getElementById('pause-all').showModal();
    else if (act === 'pause') { agent.paused = agent.paused ? '' : 'Paused by you just now';
      P.toast(agent.paused ? `${agent.name} is paused. It takes no new work until someone resumes it.` : `${agent.name} is working again.`); }
    else if (act === 'stop') { agent.doing = agent.doing.filter(job => job.id !== button.dataset.job); P.toast('Stop requested.'); }
    else if (act === 'agent-history') return P.go('history', { agent: agent.name });
    else if (act === 'setup') return P.elsewhere(`${agent.name}’s page in Agents`);
    else if (act === 'message') return P.openChat({ agent: agent.name }, 'direct', 'Work');
    else if (act === 'task-close') { P.s.tasks = P.s.tasks.filter(task => task.id !== w.task); w.task = null; P.s.paneOpen = false; P.toast(button.dataset.how); }
    P.draw();
  },
  key(key) {
    const step = { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1 }[key]; if (!step) return false;
    const rows = [...document.querySelectorAll('#view .list [data-agent], #view .list [data-task]')];
    const next = rows[Math.min(rows.length - 1, Math.max(0, rows.findIndex(row => row.getAttribute('aria-current') === 'true') + step))];
    if (next) { if (next.dataset.agent) P.s.work.agent = next.dataset.agent; else P.s.work.task = next.dataset.task; P.draw(); }
    return true;
  },
};
P.pauseAll = note => { const stopped = P.s.agents.reduce((sum, agent) => sum + agent.doing.length, 0);
  P.s.agents.forEach(agent => { agent.doing = []; }); P.s.allPaused = { note }; P.draw();
  P.toast(`All agents are paused. ${stopped} running ${stopped === 1 ? 'request was' : 'requests were'} stopped.`); };
P.resumeAll = () => { P.s.allPaused = null; P.draw(); P.toast('Agents are working again. Agents paused on their own stay paused.'); };
})();
