// A simplified Chat page, only to show where "Open in Chat" and "Ask privately" land and how you come back.
(() => {
const P = window.Proto;
const node = document.getElementById('chatview');
const channels = ['sales', 'orders', 'accounts', 'marketing'], people = ['Nova', 'Atlas', 'Ledger', 'Iris', 'Pixel'];
const itemOf = () => P.s.items.find(item => item.id === P.s.chat.item);

P.openChat = (item, kind, back = 'Needs you') => {
  const title = item.title ? (P.titleOf ? P.titleOf(item) : item.title) : '';
  P.s.chat = { kind, item: item.id, agent: item.agent, back,
    draft: kind === 'direct' && title ? `About “${title}” (${item.where ?? 'Activity'}): ` : '' };
  P.s.direct[item.agent] ??= [];
  P.drawChat();
  const box = node.querySelector('textarea'); box.focus(); box.setSelectionRange(box.value.length, box.value.length);
  node.querySelector('.msg.target')?.scrollIntoView({ block: 'center' });
};
P.closeChat = () => { P.s.chat = null; P.drawChat(); P.draw(); };

function card(item) {
  if (item.kind !== 'approval') return '';
  return `<div class="card chat-card"><small>Needs an approval</small><strong>${item.card.head}</strong>${P.fields(item.card.fields)}
    ${item.done ? `<span class="decided">✓ ${item.done.label} by you · just now</span>`
      : `<div class="btns">${P.button('Approve this action', 'approve', { primary: true })}${P.button('Reject', 'reject')}</div><p class="help">The same request as in Activity. Deciding it here removes it there.</p>`}</div>`;
}
P.drawChat = () => {
  const c = P.s.chat;
  node.hidden = !c;
  document.getElementById('rail-chat').classList.toggle('on', Boolean(c));
  document.getElementById('rail-activity').classList.toggle('on', !c);
  if (!c) return;
  const item = itemOf(), source = c.kind === 'source', place = source ? item.talk.place.slice(1) : null;
  const msgs = source ? item.talk.msgs : P.s.direct[c.agent];
  const side = `<aside class="chat-side"><h2>Chat</h2>
    <span class="chat-row"><i class="dot off"></i>For you</span>
    <h3>Channels</h3>${channels.map(name => `<span class="chat-row${name === place ? ' on' : ''}"># ${name}</span>`).join('')}
    <h3>Direct messages</h3>${people.map(name => `<span class="chat-row${!source && name === c.agent ? ' on' : ''}">${P.face(name)}${name}<i class="ai">AI</i></span>`).join('')}
    <span class="chat-row"><span class="face" style="--f:#7b8794">K</span>Kavya Nair</span><span class="chat-row"><span class="face" style="--f:#7b8794">R</span>Ravi Kumar</span></aside>`;
  const head = source ? `<span class="chat-title"><b># ${place}</b><small>with ${item.talk.see}</small></span>`
    : `<span class="chat-title">${P.face(c.agent, 'lg')}<span><b>${c.agent} <i class="ai">AI</i></b><small>${P.s.direct[c.agent].length ? '● Working on your request…' : '● Ready'} · only you and ${c.agent} are here</small></span></span>`;
  const body = msgs.length ? msgs.map(m => P.msg(m, m.target ? `${card(item)}<span class="came">You came from this request</span>` : '')).join('')
    : P.empty(`Your private chat with ${c.agent}`, 'Nobody else sees what you write here. Its owners and admins can see its work in Activity.');
  node.innerHTML = `${side}<section class="chat-main">
    <header class="chat-head"><button type="button" class="chat-return" data-act="return">‹ ${c.back}</button>${head}</header>
    <div class="chat-scroll"><div class="chat-col">${source ? '<p class="chat-day">Today</p>' : ''}${body}</div></div>
    <form class="composer"><label class="sr" for="chat-text">Message</label>
      <textarea id="chat-text" rows="2" placeholder="Message ${source ? `#${place}` : c.agent}">${P.esc(c.draft)}</textarea>
      <button type="submit" class="btn pri">Send</button></form>
    <p class="chat-note">${!source && c.draft ? 'This draft was started for you. Nothing is sent until you press Send. ' : ''}Simplified Chat page, to show where the link lands.</p></section>`;
};
node.addEventListener('click', event => {
  const act = event.target.closest('[data-act]')?.dataset.act; if (!act) return;
  if (act === 'return') return P.closeChat();
  if (act === 'approve' || act === 'reject') { P.decideApproval(itemOf(), act); P.drawChat(); }
});
node.addEventListener('input', event => { if (event.target.id === 'chat-text') P.s.chat.draft = event.target.value; });
node.addEventListener('submit', event => {
  event.preventDefault();
  const c = P.s.chat, text = c.draft.trim(); if (!text) return;
  if (c.kind === 'source') P.say(itemOf(), text); else P.s.direct[c.agent].push({ who: 'You', mine: true, time: 'just now', text: P.esc(text) });
  c.draft = ''; P.drawChat();
  node.querySelector('.chat-scroll').scrollTop = 1e6; node.querySelector('textarea').focus();
});
})();
