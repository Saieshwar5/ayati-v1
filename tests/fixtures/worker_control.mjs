import assert from 'node:assert/strict';
import { randomBytes, randomUUID } from 'node:crypto';
import { once } from 'node:events';

export function controllerToolsOnly(request) {
  // Explicit reviewed controller surface. Owner browser, memory, shell and file
  // extensions must never enter a shared managed-agent session.
  assert.deepEqual((request.tools ?? []).map(tool => tool.function?.name).sort(), [
    'ayati_control__ask_question', 'ayati_control__attach_to_task', 'ayati_control__close_task',
    'ayati_control__continue_privately', 'ayati_control__create_task', 'ayati_control__follow_up', 'ayati_control__handoff_targets',
    'ayati_control__home_panel_update_context', 'ayati_control__load_skill', 'ayati_control__open_tasks',
    'ayati_control__operations', 'ayati_control__personal_panels', 'ayati_control__propose_schedule',
    'ayati_control__read_conversation_history', 'ayati_control__refresh_home_panel', 'ayati_control__report_problem', 'ayati_control__request_action',
    'ayati_control__request_handoff', 'ayati_control__save_home_panel',
    'ayati_control__save_working_context',
    'ayati_control__search_conversation_history', 'ayati_control__send_update',
  ]);
}

export function managedGuidanceOnly(request) {
  const text = JSON.stringify(request.messages);
  assert.ok(text.includes('Managed workspace guidance:'), 'Shared work needs its own product guidance');
  for (const unavailable of ['permanent guide', 'Owner-configured role:', 'panel_write must',
    'Use panel_write or app_write', 'publish=true', 'Floating Canvas']) {
    assert.ok(!text.includes(unavailable), `Local-owner guidance leaked into shared work: ${unavailable}`);
  }
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
