// Talking about an item without leaving it: the last messages, a reply box, and the two ways into Chat.
(() => {
const P = window.Proto;
P.msg = (m, extra = '') => `<div class="msg${m.mine ? ' mine' : ''}${m.target ? ' target' : ''}">${m.mine ? '' : m.agent ? P.face(m.who) : `<span class="face" style="--f:#7b8794" aria-hidden="true">${m.who[0]}</span>`}
  <div class="bubble"><span class="by"><b>${m.mine ? 'You' : m.who}</b>${m.agent ? '<i class="ai">AI</i>' : ''}<time>${m.time}</time></span><p>${m.text}</p>${extra}</div></div>`;
const privately = item => `<button type="button" class="link" data-act="ask">Ask ${item.agent} privately</button>`;

/** The Conversation part of the pane. `draft.changes` turns the reply box into "what should change?". */
P.peek = (item, draft) => {
  if (item.outside) return `<h3>Conversation</h3><p class="note plain">You are not in ${item.outside}, so you can’t open it. As an admin you can read this request; your view is recorded in the audit log. ${privately(item)}</p>`;
  if (item.room) return `<h3>Conversation</h3><p class="note plain">This ran on a schedule in its own work room, so there is no conversation to open. ${privately(item)}</p>`;
  const talk = item.talk; if (!talk) return '';
  // The agent's own question is already the card above; the peek shows what led to it.
  const all = talk.msgs.filter(m => !m.quote), shown = draft.allTalk ? all : all.slice(-2), hidden = all.length - shown.length;
  const label = draft.changes ? 'What should change?' : talk.mine ? 'Add to this request' : `Reply in ${talk.place}`;
  const note = talk.mine ? `${item.agent} reads it at its next step.` : `Posts in ${talk.place} as you. ${talk.see} will see it.`;
  return `<div class="talk-head"><h3>Conversation · ${talk.place}</h3><button type="button" class="link" data-act="open-chat">Open in Chat ›<kbd>O</kbd></button></div>
    <div class="peek">${hidden ? `<button type="button" class="link" data-act="talk-more">Show ${hidden} earlier</button>` : ''}${shown.map(m => P.msg(m)).join('')}</div>
    ${item.done ? '' : `<div class="reply${draft.changes ? ' changes' : ''}"><label class="sr" for="reply-text">${label}</label>
      <textarea id="reply-text" rows="2" maxlength="2000" placeholder="${label}…">${P.esc(draft.reply ?? '')}</textarea>
      <div class="reply-foot"><small>${note}</small>${draft.changes ? '' : P.button(talk.mine ? 'Add' : 'Reply', 'reply', { disabled: !(draft.reply ?? '').trim() })}</div></div>
      <p class="help">${privately(item)} if you don’t want to say it in ${talk.place}.</p>`}`;
};
const post = (item, text) => item.talk.msgs.push({ who: 'You', mine: true, time: 'just now', text: P.esc(text.trim()) });
P.say = post;

/** Returns true when it handled the action. `back` is where "‹ back" in Chat returns to. */
P.talkAct = (name, item, draft, back) => {
  if (name === 'talk-more') draft.allTalk = true;
  else if (name === 'open-chat') return P.openChat(item, 'source', back), true;
  else if (name === 'ask') return P.openChat(item, 'direct', back), true;
  else if (name === 'reply') {
    post(item, draft.reply); draft.reply = ''; draft.allTalk = true;
    P.toast(item.talk.mine ? `Added. ${item.agent} reads it at its next step.` : `Posted in ${item.talk.place}. ${item.talk.see} can see it.`);
  } else return false;
  P.draw(); return true;
};
/** Typing must not redraw the pane, so only the buttons that depend on the text change. */
P.talkInput = (target, draft) => {
  if (target.id !== 'reply-text') return false;
  draft.reply = target.value;
  document.querySelectorAll('[data-act="reply"], [data-act="changes-send"]').forEach(button => { button.disabled = !target.value.trim(); });
  return true;
};
})();
