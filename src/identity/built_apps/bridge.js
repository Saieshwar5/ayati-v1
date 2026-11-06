// Cuckoo's bridge for built apps. The app runs in a sandboxed frame with no network, so
// it asks the Cuckoo page around it, by message, and Cuckoo's server decides every answer
// from the app's manifest and the person's roles.
(() => {
  const waiting = new Map();
  let next = 0;
  window.addEventListener('message', (event) => {
    if (event.source !== window.parent) return;
    const reply = event.data;
    if (!reply || reply.cuckoo !== 1 || !waiting.has(reply.id)) return;
    const { resolve, reject } = waiting.get(reply.id);
    waiting.delete(reply.id);
    if (reply.ok) resolve(reply.result);
    else reject(new Error(reply.error || 'Cuckoo could not do that.'));
  });
  const ask = (op, args) =>
    new Promise((resolve, reject) => {
      const id = ++next;
      waiting.set(id, { resolve, reject });
      window.parent.postMessage({ cuckoo: 1, id, op, ...args }, '*');
    });
  const cuckoo = {
    /** The person using the app: their name and roles in it. */
    me: () => ask('me', {}),
    /** Records in a collection the person may read, newest first; { mine: true } for their own. */
    list: (collection, options = {}) => ask('list', { collection, mine: Boolean(options.mine) }),
    get: (collection, id) => ask('get', { collection, record: id }),
    create: (collection, data) => ask('create', { collection, data }),
    /** Changes a record from the revision that was read; a newer one is refused. */
    update: (collection, id, data, revision) => ask('update', { collection, record: id, data, revision }),
    remove: (collection, id) => ask('delete', { collection, record: id }),
  };
  Object.defineProperty(window, 'cuckoo', { value: Object.freeze(cuckoo) });
})();
