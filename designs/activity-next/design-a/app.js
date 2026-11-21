// The shell: tabs, search, keyboard, the prototype's own controls.
(() => {
const P = window.Proto;
const tabs = [['needs', 'Needs you'], ['work', 'Work'], ['planned', 'Planned'], ['history', 'History'], ['insights', 'Insights']];
const view = document.getElementById('view'), search = document.getElementById('search'), guide = document.getElementById('guide');

P.reset = () => {
  P.s = { tab: 'needs', search: '', scope: 'mine', paneOpen: false, showDone: false, draft: {}, allPaused: null,
    items: P.makeItems(), arrivals: P.arrivals.map(item => ({ ...item })), agents: P.makeAgents(), tasks: P.makeTasks(), planned: P.makePlanned(), history: P.makeHistory(),
    work: { mode: 'agents', filter: 'all', agent: 'Nova', task: null }, plan: 'all',
    hist: { period: '7', agent: '', type: '', status: 'all', sel: null, open: [], more: false }, insights: { suggest: true, weekly: ['Nova'] } };
  P.selectItem(P.phone() ? null : P.s.items[0].id);
  search.value = '';
};
P.drawHead = () => {
  const count = P.needCount();
  document.getElementById('tabs').innerHTML = tabs.map(([id, label]) => `<button type="button" role="tab" class="tab${id === 'insights' ? ' quiet' : ''}" data-tab="${id}" aria-selected="${P.s.tab === id}" tabindex="${P.s.tab === id ? 0 : -1}">
    ${label}${id === 'needs' && count ? `<span class="count" aria-label="${count} waiting">${count}</span>` : ''}</button>`).join('');
  const badge = document.getElementById('rail-count'); badge.textContent = count; badge.hidden = !count;
  const banner = document.getElementById('banner'); banner.hidden = !P.s.allPaused;
  if (P.s.allPaused) banner.querySelector('span').innerHTML = `<b>All agents are paused</b> by you since 10:54 am${P.s.allPaused.note ? ` · “${P.esc(P.s.allPaused.note)}”` : ''}. No agent takes new work until an admin resumes.`;
  const current = P.views[P.s.tab];
  search.placeholder = current.search; search.disabled = Boolean(current.noSearch);
};
P.draw = () => {
  P.drawHead();
  const keep = view.querySelector('.list')?.scrollTop ?? 0, read = view.querySelector('.pane-scroll')?.scrollTop ?? 0, tab = view.dataset.tab;
  P.views[P.s.tab].render(view);
  if (tab === P.s.tab) { const list = view.querySelector('.list'); if (list) list.scrollTop = keep; }
  if (tab === P.s.tab && view.dataset.sel === String(P.sel())) { const pane = view.querySelector('.pane-scroll'); if (pane) pane.scrollTop = read; }
  view.dataset.tab = P.s.tab; view.dataset.sel = String(P.sel());
};
/** What is open in the pane, so reading position is kept only while it stays the same thing. */
P.sel = () => ({ needs: P.s.sel, work: P.s.work.mode + (P.s.work.agent ?? '') + (P.s.work.task ?? ''), history: P.s.hist.sel }[P.s.tab] ?? '');
P.go = (tab, { sel, agent } = {}) => {
  P.s.tab = tab; P.s.search = ''; search.value = '';
  if (tab === 'history') { if (agent !== undefined) P.s.hist.agent = agent; if (sel) { P.s.hist.sel = sel; P.s.hist.status = 'all'; P.s.hist.period = '7'; } }
  if (sel) P.s.paneOpen = true;
  else if (tab !== 'needs' && !(tab === 'work' && P.s.work.mode === 'tasks')) P.s.paneOpen = false;
  P.draw();
};

document.getElementById('tabs').addEventListener('click', event => { const tab = event.target.closest('[data-tab]'); if (tab) { P.s.paneOpen = false; P.go(tab.dataset.tab); } });
document.getElementById('tabs').addEventListener('keydown', event => {
  const step = { ArrowRight: 1, ArrowLeft: -1 }[event.key]; if (!step) return;
  const next = tabs[(tabs.findIndex(([id]) => id === P.s.tab) + step + tabs.length) % tabs.length][0];
  P.s.paneOpen = false; P.go(next); document.querySelector(`[data-tab="${next}"]`).focus(); event.preventDefault();
});
view.addEventListener('click', event => {
  if (!event.target.closest('details.menu')) view.querySelector('details.menu[open]')?.removeAttribute('open');
  P.views[P.s.tab].click(event.target);
});
view.addEventListener('change', event => P.views[P.s.tab].change?.(event.target));
view.addEventListener('input', event => P.views[P.s.tab].input?.(event.target));
search.addEventListener('input', () => { P.s.search = search.value; P.draw(); });

document.addEventListener('keydown', event => {
  if (event.metaKey || event.ctrlKey || event.altKey || document.querySelector('dialog[open]')) return;
  const typing = event.target.closest('input, textarea, select');
  if (event.key === 'Escape') {
    if (typing === search && search.value) { search.value = P.s.search = ''; P.draw(); }
    else if (typing) typing.blur();
    else if (P.s.paneOpen) { P.s.paneOpen = false; P.draw(); }
    return;
  }
  if (typing) return;
  if (event.key === '/') { search.focus(); return event.preventDefault(); }
  if (event.key === '?') { guide.showModal(); return event.preventDefault(); }
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  if (P.views[P.s.tab].key(key)) event.preventDefault();
});

document.getElementById('demo').addEventListener('click', event => {
  const act = event.target.closest('[data-demo]')?.dataset.demo;
  if (act === 'guide') guide.showModal();
  else if (act === 'arrive') P.arrive();
  else if (act === 'reset') { P.reset(); P.draw(); P.toast('Started over with the sample data.'); }
});
document.getElementById('banner').addEventListener('click', event => { if (event.target.closest('button')) P.resumeAll(); });
document.querySelectorAll('dialog').forEach(dialog => dialog.addEventListener('click', event => {
  if (event.target === dialog || event.target.closest('[data-close]')) dialog.close();
}));
document.getElementById('pause-form').addEventListener('submit', event => {
  event.preventDefault(); const note = document.getElementById('pause-note');
  document.getElementById('pause-all').close(); P.pauseAll(note.value.trim()); note.value = '';
});
document.querySelector('.rail').addEventListener('click', event => { const link = event.target.closest('[data-go]'); if (link) P.elsewhere(link.dataset.go); });

P.reset(); P.draw();
})();
