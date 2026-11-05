// A records screen for Cuckoo templates: a form to add one, the records you may see,
// and, for people whose role may change any record, buttons to decide each one.
// window.APP (config.js) names the collection, its fields and the decision.
const app = window.APP;
const main = document.querySelector('main');
function element(tag, props = {}, ...children) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
}
const status = element('p', { className: 'status', role: 'status' });
const list = element('ul', { className: 'records' });
let me = { name: '', roles: [] };

function value(field, input) {
  if (field.type === 'number') return input.value === '' ? null : Number(input.value);
  if (field.type === 'boolean') return input.checked;
  return input.value.trim() || null;
}

function form() {
  const inputs = app.fields.map(field => {
    const input = element('input', { name: field.name, type: { number: 'number', date: 'date', boolean: 'checkbox' }[field.type] ?? 'text', required: field.required === true });
    if (field.type === 'number') input.step = 'any';
    return [field, input];
  });
  const node = element('form', { className: 'add' },
    ...inputs.map(([field, input]) => element('label', {}, field.label, input)),
    element('button', { type: 'submit' }, app.add));
  node.onsubmit = async event => {
    event.preventDefault();
    const data = Object.fromEntries(inputs.map(([field, input]) => [field.name, value(field, input)]));
    if (app.decide) data[app.decide.field] = app.decide.start;
    try {
      await cuckoo.create(app.collection, data);
      node.reset();
      status.textContent = 'Saved.';
      await show();
    } catch (error) { status.textContent = error.message; }
  };
  return node;
}

async function show() {
  const { records } = await cuckoo.list(app.collection);
  const deciding = app.decide && me.roles.some(role => app.decide.roles.includes(role));
  list.replaceChildren(...records.map(record => {
    const parts = app.fields.map(field => record.data[field.name]).filter(v => v !== null && v !== undefined && v !== '');
    const item = element('li', {}, element('strong', {}, record.by), ' · ', parts.join(' · '));
    if (app.decide) {
      const state = record.data[app.decide.field] ?? app.decide.start;
      item.append(' · ', element('span', { className: 'state' }, state));
      if (deciding && state === app.decide.start) {
        for (const choice of app.decide.choices) {
          const button = element('button', { type: 'button' }, choice.label);
          button.onclick = async () => {
            try {
              await cuckoo.update(app.collection, record.id, { [app.decide.field]: choice.value }, record.revision);
              await show();
            } catch (error) { status.textContent = error.message; }
          };
          item.append(' ', button);
        }
      }
    }
    return item;
  }));
  if (!records.length) list.replaceChildren(element('li', { className: 'empty' }, app.empty));
}

(async () => {
  me = await cuckoo.me();
  main.append(element('p', { className: 'who' }, `${me.name} · ${me.roles.join(', ')}`), form(), status, element('h2', {}, app.listTitle), list);
  await show();
})().catch(error => { status.textContent = error.message; main.append(status); });
