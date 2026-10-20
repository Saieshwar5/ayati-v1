import { JSDOM } from 'jsdom';
import { act, createElement, StrictMode } from 'react';

export async function reactFixture(run, { url = 'http://localhost/', fetch = async () => ({ ok: true, json: async () => ({}) }) } = {}) {
  const keys = ['document', 'window', 'location', 'fetch', 'confirm', 'localStorage', 'setInterval', 'IS_REACT_ACT_ENVIRONMENT'];
  const previous = Object.fromEntries(keys.map(key => [key, globalThis[key]]));
  const dom = new JSDOM('<div id="root"></div>', { url });
  let root;
  const navigation = [];
  const settle = async () => { for (let i = 0; i < 4; i++) await new Promise(resolve => setImmediate(resolve)); };
  try {
    globalThis.window = dom.window; globalThis.document = dom.window.document;
    globalThis.localStorage = dom.window.localStorage;
    globalThis.location = { pathname: dom.window.location.pathname, search: dom.window.location.search,
      assign: path => navigation.push(path), replace: path => navigation.push(path) };
    globalThis.fetch = fetch; globalThis.confirm = () => true; globalThis.setInterval = () => 0;
    globalThis.IS_REACT_ACT_ENVIRONMENT = true;
    dom.window.HTMLDialogElement.prototype.showModal = function () { this.open = true; };
    dom.window.HTMLDialogElement.prototype.close = function () { this.open = false; this.dispatchEvent(new dom.window.Event('close')); };
    const { createRoot } = await import('react-dom/client');
    root = createRoot(document.querySelector('#root'));
    const update = async action => { await act(async () => { action?.(); await settle(); }); };
    const button = label => [...document.querySelectorAll('button')].find(node => node.textContent === label || node.getAttribute('aria-label') === label);
    const change = (node, value) => update(() => {
      Object.getOwnPropertyDescriptor(Object.getPrototypeOf(node), 'value').set.call(node, value);
      node.dispatchEvent(new dom.window.Event(node.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true }));
    });
    const click = node => update(() => { node.focus(); node.click(); });
    const submit = form => update(() => form.dispatchEvent(new dom.window.Event('submit', { bubbles: true, cancelable: true })));
    await run({ dom, navigation, button, change, click, submit, update,
      render: (Component, props = {}) => update(() => root.render(createElement(StrictMode, null, createElement(Component, props)))) });
  } finally {
    if (root) await act(async () => root.unmount());
    Object.assign(globalThis, previous); dom.window.close();
  }
}
