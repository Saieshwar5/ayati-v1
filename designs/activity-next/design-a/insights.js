// Insights: the slower questions. Can I trust an agent with more, and how is the month going?
(() => {
const P = window.Proto;
const data = P.insights;
/** Requests per day: one series, bars from a zero baseline, the busiest day labelled. */
function perDay() {
  const max = Math.max(...data.perDay.map(([, count]) => count));
  return `<figure class="chart"><figcaption>Requests per day · last 7 days</figcaption>
    <ol class="columns" style="--n:${data.perDay.length}">${data.perDay.map(([day, count]) => `<li tabindex="0" aria-label="${day}: ${count} requests">
      <span class="slot"><span class="col" style="height:${count / max * 100}%">${count === max ? `<span class="value">${count}</span>` : ''}</span></span>
      <span class="axis">${day}</span><span class="tip">${day} · ${count} requests</span></li>`).join('')}</ol></figure>`;
}
function byAgent() {
  const max = Math.max(...data.byAgent.map(([, count]) => count));
  return `<figure class="chart"><figcaption>Requests by agent · last 7 days</figcaption>
    <ul class="bars">${data.byAgent.map(([name, count]) => `<li><span class="bar-name">${P.face(name)}${name}</span><span class="track"><span class="fill" style="width:${count / max * 100}%"></span></span><span class="bar-value">${count}</span></li>`).join('')}</ul></figure>`;
}
P.views.insights = {
  search: 'Search is off on Insights', noSearch: true,
  render(root) {
    const v = P.s.insights;
    root.innerHTML = `<div class="column wide">
      ${v.suggest ? `<h3 class="group">Let agents do more</h3><div class="line suggest"><span><strong>Atlas asked 14 times to update an order, and each was approved.</strong>
        <small>You could let Atlas do it without asking. It can never go beyond your admin’s rule.</small></span>${P.button('Review in Atlas › Connections', 'rule', { primary: true })}${P.button('Keep asking', 'keep')}</div>` : ''}
      <h3 class="group">How agents are doing · last 30 days</h3>
      <div class="scroll-x"><table><thead><tr><th>Agent</th><th>Tasks done</th><th>Failed</th><th>On time</th><th>Needed a person</th><th>Your minutes</th><th>Worth a look</th><th>Weekly review</th></tr></thead>
        <tbody>${data.table.map(([name, done, failed, time, person, minutes, note]) => `<tr><th scope="row"><span class="bar-name">${P.face(name)}${name}</span></th><td>${done}</td><td>${failed}</td><td>${time}</td><td>${person}</td><td>${minutes}</td>
          <td class="look">${note || '<small>—</small>'}</td><td><button type="button" class="btn sm" data-act="weekly" data-agent="${name}" aria-pressed="${v.weekly.includes(name)}">${v.weekly.includes(name) ? 'On · turn off' : 'Turn on'}</button></td></tr>`).join('')}</tbody></table></div>
      <p class="help">Your minutes: time from reading a question or approval to answering it, at most ten minutes each.</p>
      <div class="charts">${perDay()}${byAgent()}</div>
      <details class="astable"><summary>Show as table</summary><div class="scroll-x"><table><thead><tr>${data.perDay.map(([day]) => `<th>${day}</th>`).join('')}</tr></thead><tbody><tr>${data.perDay.map(([, count]) => `<td>${count}</td>`).join('')}</tr></tbody></table></div></details>
      <h3 class="group">Goals</h3>
      ${data.goals.map(([agent, name, now, target, unit, risk]) => `<div class="line"><span><strong>${agent}: ${name}</strong><small>${now} of ${target} ${unit}${risk ? ` · at risk: ${risk}` : ''}</small>
        <span class="meter${risk ? ' risk' : ''}" role="progressbar" aria-valuemin="0" aria-valuemax="${target}" aria-valuenow="${now}"><span style="width:${Math.min(100, now / target * 100)}%"></span></span></span>${risk ? P.chip(['bad', 'At risk']) : P.chip(['done', 'On track'])}</div>`).join('')}
      <h3 class="group">Weekly reviews</h3>
      <div class="card"><small>Nova · Thu, 19 Nov</small><p><b>Done.</b> Sent 12 quotes and followed up on 9; 4 became orders.</p><p><b>Went wrong.</b> Two quotes failed because the price list was missing from the Library.</p>
        <p><b>Learned.</b> Sharma Traders always asks for a bulk discount; I now check stock before quoting.</p><p class="note plain"><b>Suggested change.</b> Let me read the price list folder automatically each morning.</p></div>
    </div>`;
  },
  click(target) {
    const button = target.closest('[data-act]'); if (!button) return;
    const v = P.s.insights, act = button.dataset.act;
    if (act === 'keep') { v.suggest = false; P.toast('It keeps asking.'); }
    else if (act === 'rule') return P.elsewhere('Atlas › Connections, at this rule');
    else if (act === 'weekly') { const name = button.dataset.agent, on = v.weekly.includes(name);
      v.weekly = on ? v.weekly.filter(entry => entry !== name) : [...v.weekly, name]; P.toast(on ? `Weekly review for ${name} is off.` : `${name} will write a review every week.`); }
    P.draw();
  },
  key: () => false,
};
})();
