// Shared pieces: faces, chips, the reading pane, the toast and the list/pane split.
(() => {
const P = window.Proto;
P.esc = text => String(text).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
P.face = (name, size = '') => `<span class="face ${size}" style="--f:${P.colours[name] ?? '#6b7280'}" aria-hidden="true">${name[0]}</span>`;
P.chip = ([tone, label]) => `<span class="chip ${tone}">${label}</span>`;
P.matches = (...text) => { const q = P.s.search.trim().toLowerCase(); return !q || text.join(' ').toLowerCase().includes(q); };
P.empty = (title, text = '') => `<div class="empty"><strong>${title}</strong>${text ? `<span>${text}</span>` : ''}</div>`;

/** The reading pane: the same shape for an approval, a question, a request or a task. */
P.pane = (d, { body = '', talk = '', bar = '', back = 'Back' } = {}) => `
  <div class="pane-scroll"><article class="pane-read">
    <button type="button" class="back" data-act="back">‹ ${back}</button>
    <div class="pane-top">${P.chip(d.chip)}${d.urgent ? P.chip(['wait', d.urgent]) : ''}</div>
    <h2>${d.title}</h2>
    <p class="facts">${P.face(d.agent)}<b>${d.agent}</b> · ${d.facts}</p>
    ${d.quote ? `<div class="card"><small>${d.agent} says</small><p>${d.quote}</p></div>` : ''}
    ${d.card ? `<div class="card"><small>${d.card.label}</small><strong>${d.card.head}</strong>${P.fields(d.card.fields)}</div>` : ''}
    ${d.lead ? `<p class="help">${d.lead}</p>` : ''}
    ${body}
    ${(d.notes ?? []).map(([tone, text]) => `<p class="note ${tone}">${text}</p>`).join('')}
    ${d.help ? `<p class="help">${d.help}</p>` : ''}
    ${talk}
    ${d.steps ? `<h3>Steps so far</h3><ol class="steps">${d.steps.map(([state, text, how]) =>
      `<li class="${state}"><i class="dot ${state}"></i><span>${text}${how ? `<small>${how}</small>` : ''}</span></li>`).join('')}</ol>` : ''}
  </article></div>
  ${bar ? `<div class="bar">${bar}</div>` : ''}`;
/** A decided item reads as decided: no "waiting" notes, and the waiting step says what you chose. */
P.settled = item => {
  if (!item.done) return item;
  const stopped = /Reject|Declin|Changes|Cancel/.test(item.done.label);
  return { ...item, urgent: '', help: '', chip: [stopped ? '' : 'done', item.done.label],
    notes: (item.notes ?? []).filter(([tone]) => tone === 'plain' || (tone === 'bad' && item.kind === 'failed')),
    steps: item.steps?.filter(([state]) => state !== 'next' || !stopped).map(step => step[0] === 'wait' ? ['ok', `${item.done.label} by you`, 'just now'] : step) };
};
P.fields = list => `<dl>${list.map(([k, v]) => `<dt>${k}</dt><dd>${v}</dd>`).join('')}</dl>`;
P.button = (label, act, { primary = false, key = '', disabled = false, data = '' } = {}) =>
  `<button type="button" class="btn${primary ? ' pri' : ''}" data-act="${act}" ${data} ${disabled ? 'disabled' : ''}>${label}${key ? `<kbd>${key}</kbd>` : ''}</button>`;

let timer;
/** One quiet line at the bottom of the frame; it never steals focus. */
P.toast = (text, action) => {
  const node = document.getElementById('toast');
  node.innerHTML = `<span>${text}</span>${action ? `<button type="button" class="link">${action.label}</button>` : ''}`;
  node.hidden = false;
  node.querySelector('button')?.addEventListener('click', () => { node.hidden = true; action.run(); });
  clearTimeout(timer); timer = setTimeout(() => { node.hidden = true; }, 7000);
};
/** Something the prototype does not go to (another page of the product). */
P.elsewhere = text => P.toast(`In the product this opens ${text}.`);
P.phone = () => matchMedia('(max-width: 860px)').matches;
})();
