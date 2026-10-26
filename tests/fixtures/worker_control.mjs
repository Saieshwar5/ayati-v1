import assert from 'node:assert/strict';
import { randomBytes, randomUUID } from 'node:crypto';
import { once } from 'node:events';

export function controllerToolsOnly(request) {
  // Explicit reviewed controller surface. Owner browser, memory, shell and file
  // extensions must never enter a shared managed-agent session.
  assert.deepEqual((request.tools ?? []).map(tool => tool.function?.name).sort(), [
    'ayati_control__ask_question', 'ayati_control__handoff_targets',
    'ayati_control__operations', 'ayati_control__personal_panels',
    'ayati_control__read_conversation_history', 'ayati_control__request_action',
    'ayati_control__request_handoff', 'ayati_control__save_home_panel',
    'ayati_control__search_conversation_history',
  ]);
}

export const secret = () => randomBytes(32).toString('base64url');
export async function machine(app, path, token, body, expected = 200, headers = {}) {
  const response = await fetch(app.control.url + path, {
    method: 'POST', headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json', Connection: 'close', ...headers },
    body: JSON.stringify(body),
  });
  assert.equal(response.status, expected, 'Unexpected machine response status');
  return response.status === 200 ? response.json() : undefined;
}
export async function executor(app) {
  const registration = { executor: randomUUID(), incarnation: randomUUID(), token: secret() };
  await machine(app, '/v1/executors/register', app.control.bootstrap, registration);
  return {
    registration,
    async reserve() {
      const request = { request: randomUUID(), token: secret() };
      const scope = await machine(app, `/v1/executors/${registration.executor}/${registration.incarnation}/reserve`, registration.token, request);
      return { url: app.control.url, scope, token: request.token, reservation: request };
    },
  };
}
export async function exited(record) {
  if (record.child.exitCode !== null) return record.child.exitCode;
  if (record.child.signalCode !== null) return null;
  let timer;
  try {
    const [code] = await Promise.race([
      once(record.child, 'exit'),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Worker did not exit: ' + record.logs)), 20000); }),
    ]);
    return code;
  } finally { clearTimeout(timer); }
}
