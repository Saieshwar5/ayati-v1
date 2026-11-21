// Needs you: one queue on the left, the open item on the right, the decision at the bottom.
(() => {
const P = window.Proto;
const groups = [['decide', 'Decide'], ['answer', 'Answer'], ['fix', 'Fix']];
const left = item => item.kind === 'group' ? item.subs.filter(sub => !sub.done).length : 1;
const titleOf = item => item.kind === 'group' ? `${left(item) || item.subs.length} × ${item.op}` : item.title;
const open = () => P.s.items.filter(item => !item.done);
const shown = () => groups.flatMap(([id]) => open().filter(item => item.group === id && P.matches(titleOf(item), item.agent, item.where)));
P.needCount = () => open().reduce((sum, item) => sum + left(item), 0);
P.needsFor = agent => open().filter(item => (item.agents ?? [item.agent]).includes(agent));
P.titleOf = titleOf;
const current = () => P.s.items.find(item => item.id === P.s.sel);
const marks = { approval: ['◇', ''], group: ['◇', ''], choice: ['?', 'q'], browser: ['?', 'q'] };

P.selectItem = id => {
  P.s.sel = id; P.s.draft = {};
  const item = current(); if (item) { item.unread = false; item.fresh = false; }
};
const row = item => {
  const [glyph, tone] = marks[item.kind] ?? ['!', 'f'];
  return `<button type="button" class="row${item.unread ? ' unread' : ''}${item.fresh ? ' fresh' : ''}${item.done ? ' done' : ''}" data-id="${item.id}" aria-current="${item.id === P.s.sel}">
    <span class="mark ${item.done ? 'd' : tone}" aria-hidden="true">${item.done ? '✓' : glyph}</span>
    <span class="row-main"><span class="row-title">${titleOf(item)}</span>
      <span class="row-meta">${item.urgent && !item.done ? P.chip(['wait', item.urgent]) : ''}${P.face(item.agent)}<span>${item.agent} · ${item.done ? item.done.label : item.where}</span></span></span>
    <span class="row-age">${item.unread ? '<i class="dot new" title="New"></i>' : ''}${item.done ? 'just now' : item.age}</span></button>`;
};

function body(item, draft) {
  if (item.kind === 'group') return `<div class="subs">${item.subs.map(sub => `<div class="sub">
    <div><strong>${sub.title}</strong>${P.fields(sub.fields)}</div>
    ${sub.done ? P.chip([sub.done === 'Approved' ? 'done' : '', sub.done]) : `<span class="sub-acts">${P.button('Approve', 'sub', { data: `data-sub="${sub.id}" data-how="Approved"` })}${P.button('Reject', 'sub', { data: `data-sub="${sub.id}" data-how="Rejected"` })}</span>`}</div>`).join('')}</div>`;
  if (item.kind === 'choice') return `<fieldset class="choices"><legend>Your answer</legend>${item.options.map(([id, label, sub]) =>
    `<label class="${draft.choice === id ? 'on' : ''}"><input type="radio" name="choice" value="${id}" ${draft.choice === id ? 'checked' : ''}><span><strong>${label}</strong><small>${sub}</small></span></label>`).join('')}</fieldset>
    <label class="field">Add a note <small>optional</small><textarea id="answer-note" rows="2" placeholder="Anything ${item.agent} should know">${P.esc(draft.note ?? '')}</textarea></label>`;
  if (item.kind === 'browser') return draft.using
    ? `<div class="browser live"><span class="browser-bar">portal.supplier.example/sign-in</span><div><strong>You are using ${item.agent}’s browser</strong><span>Sign in here. ${item.agent} sees nothing you type into password fields.</span></div></div>`
    : `<div class="browser"><span class="browser-bar">portal.supplier.example/sign-in</span><div><strong>Supplier portal · Sign in</strong><span>This is the page ${item.agent} is stuck on.</span></div></div>`;
  if (item.kind === 'unconfirmed') return `<fieldset class="choices"><legend>What happened?</legend>
    <label class="${draft.happened === 'yes' ? 'on' : ''}"><input type="radio" name="happened" value="yes" ${draft.happened === 'yes' ? 'checked' : ''}><span><strong>It went through</strong><small>You can see it in Gmail’s Sent folder.</small></span></label>
    <label class="${draft.happened === 'no' ? 'on' : ''}"><input type="radio" name="happened" value="no" ${draft.happened === 'no' ? 'checked' : ''}><span><strong>It didn’t go through</strong><small>${item.agent} may send it again.</small></span></label></fieldset>`;
  return '';
}
function bar(item, draft) {
  const talk = P.button('Open conversation', 'talk');
  if (item.done) return `<span class="decided">✓ ${item.done.label} by you · just now. It can’t be changed here.</span>${P.button('See it in History', 'history')}`;
  if (item.kind === 'approval') return P.button('Approve this action', 'approve', { primary: true, key: 'A' }) + P.button('Reject', 'reject', { key: 'R' }) + P.button('Request changes', 'changes', { key: 'C' }) + talk;
  if (item.kind === 'group') return P.button(`Approve all ${left(item)}`, 'approve', { primary: true, key: 'A' }) + talk;
  if (item.kind === 'choice') return P.button('Send answer', 'answer', { primary: true, disabled: !draft.choice }) + P.button('I can’t answer', 'decline') + talk;
  if (item.kind === 'browser') return draft.using ? P.button('Done, give back', 'giveback', { primary: true }) + P.button('I can’t do it', 'decline')
    : P.button('Take over the browser', 'takeover', { primary: true }) + talk;
  if (item.kind === 'connection') return P.button('Reconnect Zoho Books', 'reconnect', { primary: true }) + '<span class="bar-note">Opens Settings → Connections</span>';
  if (item.kind === 'failed') return P.button('Mark reviewed', 'reviewed', { primary: true }) + P.button('Make this a task', 'totask') + P.button('Discuss in chat', 'talk') + talk;
  if (item.kind === 'unconfirmed') return P.button('Save answer', 'settle', { primary: true, disabled: !draft.happened }) + '<span class="bar-note">Saving lets Gmail writes run again. History shows who answered.</span>';
  return P.button('Mark done', 'taskdone', { primary: true }) + P.button('Cancel task', 'taskcancel') + talk;
}

function render(root) {
  const list = shown(), done = P.s.items.filter(item => item.done);
  if (P.s.sel && !current()) P.s.sel = null;
  const item = current();
  const queue = `<p class="away"><span>Since yesterday 5:54 pm · 34 requests · 2 failed</span><button type="button" class="link" data-act="away">See them</button></p>`
    + (list.length ? groups.map(([id, label]) => {
        const rows = list.filter(entry => entry.group === id);
        return rows.length ? `<h3 class="group">${label} · ${rows.reduce((sum, entry) => sum + left(entry), 0)}</h3>${rows.map(row).join('')}` : '';
      }).join('')
      : P.s.search.trim() ? P.empty(`Nothing matches “${P.esc(P.s.search)}”`) : P.empty('Nothing needs you right now', 'Approvals, questions and things to fix appear here.'))
    + (done.length ? `<button type="button" class="group fold" data-act="toggle-done" aria-expanded="${P.s.showDone}">Decided today · ${done.length} <span>${P.s.showDone ? 'Hide' : 'Show'}</span></button>${P.s.showDone ? done.map(row).join('') : ''}` : '');
  const pane = item ? P.pane({ ...P.settled(item), title: titleOf(item) }, { body: body(item, P.s.draft), bar: bar(item, P.s.draft), back: 'Needs you' })
    : `<div class="pane-scroll">${open().length ? P.empty('Choose something on the left', 'Or press J to open the first one.') : P.empty('You are all caught up', `${done.length} decided today. New requests appear in the list by themselves.`)}</div>`;
  root.innerHTML = `<div class="split${P.s.paneOpen && item ? ' open' : ''}"><div class="list" id="queue">${queue}</div><section class="pane" aria-label="Details">${pane}</section></div>`;
}

/** The item leaves the list, is written to History, and the next one opens. */
function finish(item, label, said) {
  const before = shown(), at = before.indexOf(item);
  item.done = { label };
  P.s.history.unshift({ id: `h-${item.id}`, day: 'Today', time: 'Just now', agent: item.agent, type: 'chat', title: titleOf(item), meta: `${label} by you · ${item.where}`,
    status: [label === 'Rejected' || label === 'Declined' ? '' : 'done', label], item: item.id });
  if (item.task) P.s.tasks = P.s.tasks.filter(task => task.id !== item.task);
  const after = shown(), next = after[Math.min(Math.max(at, 0), after.length - 1)];
  P.selectItem(next?.id ?? null);
  if (!next) P.s.paneOpen = false;
  P.toast(said, { label: 'See it in History', run: () => P.go('history', { sel: `h-${item.id}` }) });
}
const move = step => {
  const list = shown(); if (!list.length) return;
  const at = list.findIndex(item => item.id === P.s.sel);
  P.selectItem(list[Math.min(list.length - 1, Math.max(0, at + step))].id);
  P.draw(); document.querySelector('#queue [aria-current="true"]')?.scrollIntoView({ block: 'nearest' });
};
function act(name, target) {
  const item = current(), draft = P.s.draft;
  if (name === 'back') { P.s.paneOpen = false; return P.draw(); }
  if (name === 'toggle-done') { P.s.showDone = !P.s.showDone; return P.draw(); }
  if (name === 'away') return P.go('history');
  if (!item) return;
  if (name === 'talk') return P.elsewhere(`the conversation in ${item.where.startsWith('#') ? item.where : 'Chat'}, at this message`);
  if (name === 'history') return P.go('history', { sel: `h-${item.id}` });
  if (item.done) return;
  if (item.kind === 'approval' && ['approve', 'reject', 'changes'].includes(name))
    finish(item, { approve: 'Approved', reject: 'Rejected', changes: 'Changes requested' }[name], item.said[name]);
  else if (item.kind === 'group' && name === 'approve') {
    const count = left(item); item.subs.forEach(sub => { sub.done ??= 'Approved'; });
    finish(item, `${count} approved`, `${count} approved, each on its own. ${item.agent} is sending the refunds.`);
  } else if (name === 'sub') {
    item.subs.find(sub => sub.id === target.dataset.sub).done = target.dataset.how;
    if (!left(item)) finish(item, 'Decided', `All three decided. ${item.agent} is doing the approved ones.`);
  } else if (name === 'answer') {
    const choice = item.options.find(([id]) => id === draft.choice)[1];
    finish(item, `Answered “${choice}”`, `Answer sent. ${item.agent} carries on with ${choice}.`);
  } else if (name === 'decline') finish(item, 'Declined', `${item.agent} was told you can’t help with this one. It stops and tells whoever asked.`);
  else if (name === 'takeover') draft.using = true;
  else if (name === 'giveback') finish(item, 'Signed in', `Browser given back. ${item.agent} is downloading the invoice.`);
  else if (name === 'reconnect') finish(item, 'Reconnected', 'Zoho Books is connected again. Nova and Ledger can use it.');
  else if (name === 'reviewed') finish(item, 'Marked reviewed', 'Marked reviewed. Nothing was retried or changed.');
  else if (name === 'totask') return P.toast('Saved as a task. It stays in Work → Tasks until it is closed.');
  else if (name === 'settle') finish(item, draft.happened === 'yes' ? 'Recorded as sent' : 'Recorded as not sent', 'Answer saved. Gmail writes can run again.');
  else if (name === 'taskdone') finish(item, 'Marked done', 'Task marked done.');
  else if (name === 'taskcancel') finish(item, 'Cancelled', 'Task cancelled.');
  P.draw();
}

P.views.needs = {
  search: 'Search what needs you', render,
  click(target) {
    const picked = target.closest('.row');
    if (picked) { P.selectItem(picked.dataset.id); P.s.paneOpen = true; return P.draw(); }
    const button = target.closest('[data-act]'); if (button && !button.disabled) act(button.dataset.act, button);
  },
  change(target) {
    if (target.name === 'choice') P.s.draft.choice = target.value;
    else if (target.name === 'happened') P.s.draft.happened = target.value;
    else return;
    P.draw();
  },
  input(target) { if (target.id === 'answer-note') P.s.draft.note = target.value; },
  key(key) {
    if (key === 'j' || key === 'ArrowDown') return move(1), true;
    if (key === 'k' || key === 'ArrowUp') return move(-1), true;
    const item = current(); if (!item || item.done) return false;
    const name = { a: 'approve', r: 'reject', c: 'changes' }[key];
    if (name && (item.kind === 'approval' || (item.kind === 'group' && name === 'approve'))) return act(name), true;
    return false;
  },
};
/** A request that arrives while you are reading: it joins its group and nothing else moves. */
P.arrive = () => {
  const item = P.s.arrivals.shift(); if (!item) return P.toast('No more sample requests. Start over to see them again.');
  P.s.items.push(item);
  if (P.s.tab === 'needs') P.draw(); else P.drawHead();
  P.toast(`New: ${item.agent} ${item.group === 'decide' ? 'asks to' : 'asks'} “${item.title}”. Your place in the list did not move.`,
    { label: 'Open it', run: () => { P.selectItem(item.id); P.s.paneOpen = true; P.go('needs'); } });
};
})();
