// History: the log first. Totals are one line, and a row opens beside the list.
(() => {
const P = window.Proto;
const totals = {
  7: ['Last 7 days', '214 requests', '198 done', 6, '163 actions (141 automatic, 22 approved)', '38 schedule runs'],
  today: ['Today', '50 requests', '44 done', 2, '31 actions (27 automatic, 4 approved)', '5 schedule runs'],
};
const statuses = [['all', 'All'], ['wrong', 'Went wrong'], ['wait', 'Waiting'], ['done', 'Done']];
const types = [['', 'Any type'], ['chat', 'Chat'], ['schedule', 'Schedules'], ['trigger', 'Triggers'], ['panel', 'Panel updates']];
const wrong = entry => entry.status[0] === 'bad' || entry.status[1] === 'Unconfirmed';
function rows() {
  const h = P.s.hist;
  return P.s.history.filter(entry => (h.period !== 'today' || entry.day === 'Today') && (!h.agent || entry.agent === h.agent) && (!h.type || entry.type === h.type)
    && (h.status === 'all' || (h.status === 'wrong' ? wrong(entry) : h.status === 'wait' ? entry.status[0] === 'wait' && !wrong(entry) : entry.status[0] === 'done'))
    && P.matches(entry.title, entry.agent, entry.meta));
}
/** A row made from something in Needs you shows that item's own details. */
function detail(entry) {
  const item = entry.item && P.s.items.find(found => found.id === entry.item);
  const base = item ? { ...P.settled(item), title: P.titleOf(item), urgent: '' } : { facts: `${entry.meta} · ${entry.day.toLowerCase()} ${entry.time}`, steps: [['ok', 'Read the message'], ['ok', 'Replied']], ...entry.detail };
  return { ...base, chip: entry.status[1] === 'Show all' ? ['done', 'Done'] : item?.done ? base.chip : entry.status, title: base.title ?? entry.title, agent: entry.agent };
}
const row = entry => {
  const folded = entry.fold && !P.s.hist.open.includes(entry.id);
  return `<button type="button" class="hrow${entry.light ? ' light' : ''}" data-id="${entry.id}" aria-current="${entry.id === P.s.hist.sel}">
    <time>${entry.time}</time><span class="hwho">${P.face(entry.agent)}${entry.agent}</span>
    <span class="row-main"><span class="row-title">${entry.quoted ? `“${entry.title}”` : entry.title}</span><span class="row-meta"><span>${entry.meta}</span></span>
      ${(entry.lines ?? []).map(line => `<span class="row-meta"><span>→ ${line}</span></span>`).join('')}</span>
    ${entry.fold ? `<span class="chip" data-act="fold" data-id="${entry.id}">${folded ? 'Show all' : 'Hide'}</span>` : P.chip(entry.status)}</button>
    ${entry.fold && !folded ? `<div class="folded">${entry.fold.map(time => `<div><time>${time}</time><span>Updated panel: Sales today</span>${P.chip(['done', 'Done'])}</div>`).join('')}<div><small>and 8 more today</small></div></div>` : ''}`;
};

P.views.history = {
  search: 'Search history',
  render(root) {
    const h = P.s.hist, list = rows(), total = totals[h.period];
    const days = [...new Set(list.map(entry => entry.day))];
    const chosen = list.find(entry => entry.id === h.sel);
    const names = [...new Set(P.s.agents.map(agent => agent.name))];
    const linked = chosen?.item && P.s.items.find(item => item.id === chosen.item);
    const pane = chosen ? P.pane(detail(chosen), { back: 'History', talk: linked ? P.peek(linked, P.s.draft) : '', bar: (P.s.items.some(item => item.id === chosen.item && !item.done) ? P.button('Open in Needs you', 'open-item', { primary: true }) : '') + (chosen.status[0] === 'work' ? P.button('Stop request', 'stop') : '') + (linked ? '' : P.button('Open in Chat', 'talk') + P.button(`Ask ${chosen.agent} privately`, 'ask')) + P.button('Make this a task', 'totask') })
      : `<div class="pane-scroll">${P.empty('Choose a request', 'Every step it took, how each step was allowed, and the reply show here.')}</div>`;
    root.innerHTML = `<div class="stack"><div class="toolbar">
        <select data-act="period" aria-label="Period"><option value="7" ${h.period === '7' ? 'selected' : ''}>Last 7 days</option><option value="today" ${h.period === 'today' ? 'selected' : ''}>Today</option></select>
        <select data-act="agent" aria-label="Agent"><option value="">Any agent</option>${names.map(name => `<option ${h.agent === name ? 'selected' : ''}>${name}</option>`).join('')}</select>
        <select data-act="type" aria-label="Type">${types.map(([id, label]) => `<option value="${id}" ${h.type === id ? 'selected' : ''}>${label}</option>`).join('')}</select>
        <span class="pills">${statuses.map(([id, label]) => `<button type="button" class="pill" data-act="status" data-status="${id}" aria-pressed="${h.status === id}">${label}</button>`).join('')}</span>
        <span class="grow"></span>${P.button('Export CSV', 'export')}</div>
      <p class="summary"><b>${total[0]}</b> · ${total[1]} · ${total[2]} · <button type="button" class="link" data-act="status" data-status="wrong">${total[3]} need a look</button> · ${total[4]} · ${total[5]}
        <button type="button" class="link" data-act="insights">Charts and trends are in Insights ›</button></p>
      <div class="split wide${P.s.paneOpen && chosen ? ' open' : ''}"><div class="list">
        ${days.map(day => `<h3 class="group">${day}</h3>${list.filter(entry => entry.day === day).map(row).join('')}`).join('') || P.empty('No agent work matches these filters')}
        ${!h.more && h.period === '7' ? `<div class="more">${P.button('Show older', 'older')}</div>` : ''}</div>
        <section class="pane" aria-label="Request details">${pane}</section></div></div>`;
  },
  click(target) {
    const h = P.s.hist, button = target.closest('[data-act]');
    if (button?.dataset.act === 'fold') { h.open = h.open.includes(button.dataset.id) ? h.open.filter(id => id !== button.dataset.id) : [...h.open, button.dataset.id]; return P.draw(); }
    const picked = target.closest('.hrow');
    if (picked && !button) { h.sel = picked.dataset.id; P.s.draft = {}; P.s.paneOpen = true; return P.draw(); }
    const entry = P.s.history.find(found => found.id === h.sel), linked = entry?.item && P.s.items.find(item => item.id === entry.item);
    if (button && linked && P.talkAct(button.dataset.act, linked, P.s.draft, 'History')) return;
    if (!button || button.tagName === 'SELECT') return;
    const act = button.dataset.act;
    if (act === 'status') h.status = button.dataset.status;
    else if (act === 'back') P.s.paneOpen = false;
    else if (act === 'older') { h.more = true; P.s.history.push(...P.olderHistory()); }
    else if (act === 'insights') return P.go('insights');
    else if (act === 'export') return P.toast('Downloads the filtered requests as a CSV file (at most 5,000).');
    else if (act === 'talk') return P.elsewhere('the conversation in Chat, at this message');
    else if (act === 'ask') return P.openChat({ agent: entry.agent, title: entry.title, where: entry.meta }, 'direct', 'History');
    else if (act === 'totask') return P.toast('Saved as a task. It stays in Work → Tasks until it is closed.');
    else if (act === 'stop') return P.toast('Stop requested.');
    else if (act === 'open-item') { P.selectItem(P.s.history.find(entry => entry.id === h.sel).item); P.s.paneOpen = true; return P.go('needs'); }
    P.draw();
  },
  input(target) { P.talkInput(target, P.s.draft); },
  change(target) { if (['period', 'agent', 'type'].includes(target.dataset.act)) { P.s.hist[target.dataset.act] = target.value; P.draw(); } },
  key(key) {
    const step = { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1 }[key]; if (!step) return false;
    const list = rows(), at = list.findIndex(entry => entry.id === P.s.hist.sel);
    const next = list[Math.min(list.length - 1, Math.max(0, at + step))];
    if (next) { P.s.hist.sel = next.id; P.draw(); document.querySelector('.hrow[aria-current="true"]')?.scrollIntoView({ block: 'nearest' }); }
    return true;
  },
};
})();
