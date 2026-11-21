// Planned: everything agents will do on their own, in time order from now.
(() => {
const P = window.Proto;
const kinds = { schedule: 'Schedule', trigger: 'Trigger', follow_up: 'Follow-up', due: 'Due' };
const filters = [['all', 'All'], ['schedule', 'Schedules'], ['trigger', 'Triggers'], ['follow_up', 'Follow-ups']];
const fits = (item, filter) => filter === 'all' || item.kind === filter || (filter === 'follow_up' && item.kind === 'due');
const mine = item => P.s.scope === 'all' || !P.s.agents.find(agent => agent.name === item.agent)?.owner;

function actions(item) {
  const edit = `<button type="button" class="link" data-act="edit" data-id="${item.id}">Edit in ${item.agent}</button>`;
  const id = `data-id="${item.id}"`;
  if (item.kind === 'follow_up') return P.button('Cancel follow-up', 'cancel', { data: id }) + `<button type="button" class="link" data-act="task" ${id}>Open task</button>`;
  if (item.kind === 'due') return `<button type="button" class="link" data-act="task" ${id}>Open task</button>`;
  if (!item.paused) return P.button('Pause', 'pause', { data: id }) + edit;
  if (item.kind === 'trigger') return `<button type="button" class="link" data-act="edit" ${id}>Review and resume in ${item.agent}</button>`;
  return (item.by === 'you' ? P.button('Resume', 'resume', { data: id }) : `<small>Only ${item.by} can resume it.</small>`) + edit;
}
const row = item => `<div class="prow${item.paused ? ' off' : ''}"><time>${!item.paused && item.time ? item.time : '—'}</time>${P.face(item.agent)}
  <span class="row-main"><span class="row-title">${item.title}</span>
    <span class="row-meta"><span>${kinds[item.kind]} · ${item.agent} · ${item.paused ? 'Paused' : item.cadence}${item.by ? ` · set up by ${item.by}` : ''}</span></span>
    ${item.warn && !item.paused ? `<span class="row-meta warn">${item.warn}</span>` : ''}</span>
  <span class="acts">${actions(item)}</span></div>`;

P.views.planned = {
  search: 'Search planned work',
  render(root) {
    const list = P.s.planned.filter(item => mine(item) && fits(item, P.s.plan) && P.matches(item.title, item.agent));
    const timed = list.filter(item => item.kind !== 'trigger' && !item.paused);
    const days = [...new Set(timed.map(item => item.day))].map(day => [day, timed.filter(item => item.day === day)]);
    const groups = [...days, ['Paused', list.filter(item => item.kind !== 'trigger' && item.paused)], ['Always on', list.filter(item => item.kind === 'trigger')]].filter(([, rows]) => rows.length);
    const running = P.s.agents.reduce((sum, agent) => sum + agent.doing.length, 0);
    root.innerHTML = `<div class="stack"><div class="toolbar"><span class="pills">${filters.map(([id, label]) =>
        `<button type="button" class="pill" data-act="plan" data-plan="${id}" aria-pressed="${P.s.plan === id}">${label}</button>`).join('')}</span><span class="grow"></span>${P.scopePick()}</div>
      <div class="column"><div class="nowline"><span>Now · 10:54 am</span><button type="button" class="link" data-act="work">${running} in progress · see Work</button></div>
        ${groups.map(([title, rows]) => `<h3 class="group">${title}</h3>${rows.map(row).join('')}`).join('') || P.empty('Nothing matches', 'Schedules and triggers are set up on an agent’s Automations tab.')}</div></div>`;
  },
  click(target) {
    const button = target.closest('[data-act]'); if (!button) return;
    const act = button.dataset.act, item = P.s.planned.find(entry => entry.id === button.dataset.id);
    if (act === 'plan') P.s.plan = button.dataset.plan;
    else if (act === 'scope') P.s.scope = button.dataset.scope;
    else if (act === 'work') return P.go('work');
    else if (act === 'edit') return P.elsewhere(`${item.agent}’s Automations tab`);
    else if (act === 'task') { P.s.work.mode = 'tasks'; P.s.work.task = item.task ?? 't-follow'; P.s.paneOpen = true; return P.go('work'); }
    else if (act === 'cancel') { P.s.planned = P.s.planned.filter(entry => entry !== item); P.toast(`Follow-up for “${item.title}” cancelled; the task stays open.`); }
    else if (act === 'resume') { item.paused = false; P.toast(`${item.title} is back on.`); }
    else if (act === 'pause') {
      item.paused = true;
      if (item.kind === 'trigger') P.toast(`${item.title} is paused. Resume it on ${item.agent}’s Automations tab, after checking its access.`);
      else if (item.by === 'you') P.toast(`${item.title} is paused.`, { label: 'Undo', run: () => { item.paused = false; P.draw(); } });
      else P.toast(`${item.title} is paused. Only ${item.by} can resume it.`);
    }
    P.draw();
  },
  key: () => false,
};
})();
