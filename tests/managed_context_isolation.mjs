import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { controllerToolsOnly, managedGuidanceOnly } from './fixtures/worker_control.mjs';

for (const controlled of [false, true]) {
  test(`managed ${controlled ? 'pooled controlled' : 'tool-free compatibility'} Goose ignores host project hints`, {
    skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 90000,
  }, async t => {
    const app = await workerFixture(t, controlled ? { mode: 'pooled', control: true, leaseSeconds: 15 } : {});
    // Owned fixture ancestors deliberately contain unrelated host instructions.
    // Do not modify the real repository or user's global instructions.
    await writeFile(join(app.data, 'AGENTS.md'), 'PRIVATE_HOST_AGENT_HINT: this host project is unrelated to the business request.\n');
    await writeFile(join(app.data, '.goosehints'), 'PRIVATE_HOST_GOOSE_HINT: this host project is unrelated to the business request.\n');
    if (controlled) await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
    else await app.startWorker();
    await app.done(await app.send('Summarize the current task using only its supplied context.'));
    assert.equal(app.provider.requests.length, 1);
    const request = app.provider.requests[0];
    const text = JSON.stringify(request.messages);
    assert.ok(!text.includes('PRIVATE_HOST_AGENT_HINT'), 'Ancestor AGENTS.md entered managed model input');
    assert.ok(!text.includes('PRIVATE_HOST_GOOSE_HINT'), 'Ancestor .goosehints entered managed model input');
    assert.ok(!text.includes('Local OpenPencil connection'), 'Repository instructions entered managed model input');
    managedGuidanceOnly(request);
    if (controlled) controllerToolsOnly(request);
    else assert.deepEqual(request.tools ?? [], []);
    await writeFile(join(app.data, 'context-isolation.json'), JSON.stringify({
      passed: true, controlled, ancestor_agents_excluded: true, ancestor_goosehints_excluded: true,
      managed_guidance: true, model_requests: 1, tools: (request.tools ?? []).map(tool => tool.function.name),
    }, null, 2));
    app.pass('Actual managed Goose input excludes owned host hints and repository guidance while preserving its exact tool surface');
  });
}
