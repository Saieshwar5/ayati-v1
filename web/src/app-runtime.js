(() => {
  'use strict';
  const channel = new MessageChannel();
  const pending = new Map();
  let sequence = 0;
  channel.port1.onmessage = event => {
    const reply = event.data;
    const call = pending.get(reply?.id);
    if (!call) return;
    pending.delete(reply.id); clearTimeout(call.timer);
    if (reply.error) call.reject(new Error(reply.error)); else call.resolve(reply.result);
  };
  const call = command => new Promise((resolve, reject) => {
    const id = String(++sequence);
    const timer = setTimeout(() => { pending.delete(id); reject(new Error('Record response was not acknowledged. Refresh the records before repeating an operation.')); }, 30000);
    pending.set(id, { resolve, reject, timer }); channel.port1.postMessage({ id, command });
  });
  Object.defineProperty(window, 'ayati', { value: Object.freeze({ records: Object.freeze({
    list: (collection, after = null) => call({ action: 'list', collection, after }),
    save: (collection, values, id = null, expectedRevision = 0, requestId = crypto.randomUUID()) => call({ action: 'save', collection, values, id, expected_revision: expectedRevision, request_id: requestId }),
    remove: (collection, id, expectedRevision, requestId = crypto.randomUUID()) => call({ action: 'delete', collection, id, expected_revision: expectedRevision, request_id: requestId }),
  }) }), writable: false });
  window.parent.postMessage({ type: 'ayati-app-ready', document_id: document.currentScript.dataset.document }, '*', [channel.port2]);
})();
