import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const helper = fileURLToPath(new URL(
  '../vendor/openclaw/scripts/lib/release-publish-children.sh', import.meta.url,
));

for (const name of ['dispatch_linux_mirror', 'dispatch_linux_release_assets']) {
  test(`${name} refuses desktop publication before any remote call`, () => {
    const result = spawnSync('bash', ['-c',
      'source "$1"; gh() { printf remote_call; return 99; }; "$2"',
      'desktop-release-test', helper, name,
    ], {
      encoding: 'utf8',
      env: { PATH: process.env.PATH, GITHUB_REF: 'refs/tags/v2026.9.7',
        PARENT_WORKFLOW_SHA: 'test-source' },
    });
    assert.equal(result.error, undefined);
    assert.equal(result.status, 1);
    assert.equal(result.stdout, '');
    assert.match(result.stderr, /Desktop app releases are disabled in Ayati/);
  });
}
