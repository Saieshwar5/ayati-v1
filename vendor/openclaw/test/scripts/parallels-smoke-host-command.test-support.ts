import { spawn } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { afterAll, afterEach, beforeAll } from "vitest";
import { run } from "../../scripts/e2e/parallels/common.ts";
import { scriptProcessEntrypoints } from "../../scripts/script-process-runtime.test-support.js";
import {
  resolveRuntimeWorkerArgv,
  resolveRuntimeWorkerUrl,
} from "../../src/infra/runtime-worker-url.js";
import {
  fixtureReceiptClientSource,
  openFixtureReceiptChannel,
  type FixtureReceiptChannel,
} from "../helpers/fixture-receipts.js";
import { useAutoCleanupTempDirTracker } from "../helpers/temp-dir.js";
import { testNodeExecPath } from "./parallels-smoke-command.test-support.js";

const pendingChildCompletions = new Set<Promise<unknown>>();

export const tempDirs = useAutoCleanupTempDirTracker((cleanup) =>
  afterEach(async () => {
    // A timed-out body must settle its native runner before removing PID files.
    await Promise.allSettled(pendingChildCompletions);
    cleanup();
  }),
);

let receipts: FixtureReceiptChannel;
beforeAll(async () => {
  receipts = await openFixtureReceiptChannel();
});
afterAll(async () => {
  await receipts.close();
});

export function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

// The host wrapper escalates then settles; it exposes no foreign-PID reap signal.
export async function waitForProcessGone(pid: number, signal: AbortSignal): Promise<void> {
  while (isProcessAlive(pid)) {
    await delay(5, undefined, { signal }).catch((error: unknown) => {
      throw new Error(`condition was not met before timeout: process ${pid} is still alive`, {
        cause: error,
      });
    });
  }
}

function waitForProcessClose(
  child: ReturnType<typeof spawn>,
): Promise<{ code: number | null; signal: NodeJS.Signals | null }> {
  const closed = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>(
    (resolve, reject) => {
      child.once("error", reject);
      child.once("close", (code, signal) => resolve({ code, signal }));
    },
  ).finally(() => {
    pendingChildCompletions.delete(closed);
  });
  pendingChildCompletions.add(closed);
  return closed;
}

export function runNode(source: string, options: NonNullable<Parameters<typeof run>[2]> = {}) {
  return run(testNodeExecPath, ["-e", source], { quiet: true, ...options });
}

export function drainableProcessTreeScript(delayMs: number): string {
  const descendantScript = `const { writeFileSync } = require('node:fs'); process.on('SIGTERM', () => setTimeout(() => { writeFileSync(process.env.DRAIN_FILE, 'drained'); process.exit(0); }, ${delayMs})); writeFileSync(process.env.READY_FILE, 'ready'); setInterval(() => {}, 1000);`;
  return `
const { spawn } = require('node:child_process');
const { existsSync } = require('node:fs');
process.on('SIGTERM', () => process.exit(0));
let started = false;
const start = () => {
  if (started || !existsSync(process.env.DEADLINE_FILE)) return;
  started = true;
  clearInterval(probe);
  spawn(process.execPath, ['-e', ${JSON.stringify(descendantScript)}], { env: process.env, stdio: 'ignore' });
};
const probe = setInterval(start, 5);
start();
setInterval(() => {}, 1000);
`;
}

export function writeDrainDeadlinePreload(tempDir: string, timeoutMs = 100): string {
  const preload = join(tempDir, "deadline.cjs");
  writeFileSync(
    preload,
    `
const fs = require('node:fs');
const path = require('node:path');
const owner = path.join(${JSON.stringify(tempDir)}, 'wrapper.pid');
try { fs.writeFileSync(owner, String(process.pid), { flag: 'wx' }); }
catch (error) { if (error.code !== 'EEXIST') throw error; }
// NODE_OPTIONS is inherited: only the first process (the wrapper) owns this gate.
if (fs.readFileSync(owner, 'utf8') === String(process.pid)) {
  const schedule = globalThis.setTimeout;
  globalThis.setTimeout = (callback, ms, ...args) => {
    if (ms !== ${timeoutMs}) return schedule(callback, ms, ...args);
    globalThis.setTimeout = schedule;
    return schedule(() => {
      let released = false;
      let probe;
      const release = () => {
        if (released || !fs.existsSync(process.env.READY_FILE)) return;
        released = true;
        clearInterval(probe);
        callback(...args);
      };
      probe = setInterval(release, 5);
      fs.writeFileSync(process.env.DEADLINE_FILE, 'elapsed');
      release();
    }, ms);
  };
}
`,
  );
  return preload;
}

const SIGNAL_GRANDCHILD_SCRIPT = `const { writeFileSync } = require('node:fs'); writeFileSync(process.env.OPENCLAW_TEST_GRANDCHILD_PID, String(process.pid)); process.on('SIGTERM', () => {}); setInterval(() => {}, 1000);`;

export const SIGNAL_PARENT_SCRIPT = `const { spawn } = require('node:child_process'); const { writeFileSync } = require('node:fs'); spawn(process.execPath, ['-e', ${JSON.stringify(SIGNAL_GRANDCHILD_SCRIPT)}], { env: process.env, stdio: 'ignore' }); writeFileSync(process.env.OPENCLAW_TEST_READY_FILE, 'ready'); process.on('SIGTERM', () => process.exit(0)); setInterval(() => {}, 1000);`;

export function createSignaledHostCommandFixture() {
  const tempDir = tempDirs.make("openclaw-parallels-host-command-signal-");
  const runnerPath = join(tempDir, "runner.mjs");
  const readyPath = join(tempDir, "ready");
  const parentPidPath = join(tempDir, "parent.pid");
  const grandchildPidPath = join(tempDir, "grandchild.pid");
  const hostCommandUrl = resolveRuntimeWorkerUrl(scriptProcessEntrypoints.parallelsHostCommand);
  const parentScript = `${fixtureReceiptClientSource(receipts.endpoint)}
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
process.on("SIGTERM", () => process.exit(0));
writeFileSync(${JSON.stringify(parentPidPath)}, String(process.pid));
const child = spawn(process.execPath, ["-e", ${JSON.stringify(`${SIGNAL_GRANDCHILD_SCRIPT} process.send("ready");`)}], {
  env: process.env,
  stdio: ["ignore", "ignore", "ignore", "ipc"],
});
child.once("message", () => {
  writeFileSync(process.env.OPENCLAW_TEST_READY_FILE, "ready");
  sendReceipt(process.env.OPENCLAW_TEST_READY_FILE, "ready");
});
setInterval(() => {}, 1000);`;
  writeFileSync(
    runnerPath,
    `import { run } from ${JSON.stringify(hostCommandUrl.href)};
run(process.execPath, ['--input-type=module', '-e', ${JSON.stringify(parentScript)}], {
  check: false,
  env: { ...process.env, OPENCLAW_TEST_GRANDCHILD_PID: ${JSON.stringify(grandchildPidPath)}, OPENCLAW_TEST_READY_FILE: ${JSON.stringify(readyPath)} },
  quiet: true,
  timeoutMs: 30_000,
});`,
  );
  const runner = spawn(
    testNodeExecPath,
    [...resolveRuntimeWorkerArgv(hostCommandUrl, testNodeExecPath).slice(0, -1), runnerPath],
    { cwd: process.cwd(), detached: true, stdio: "ignore" },
  );
  const closed = waitForProcessClose(runner);
  const ready = Promise.race([
    receipts.waitFor(readyPath, "ready"),
    closed.then(() => {
      // PID publication precedes the parent's ready record; receipt delivery is unordered.
      if (!existsSync(readyPath) || !existsSync(grandchildPidPath)) {
        throw new Error("condition was not met before timeout");
      }
    }),
  ]);
  return { grandchildPidPath, parentPidPath, readyPath, runner, ready, closed };
}

export function forceKillSignaledFixture(
  runnerPid: number,
  grandchildPid: number,
  processGroup: boolean,
): void {
  if (runnerPid && isProcessAlive(runnerPid)) {
    process.kill(processGroup ? -runnerPid : runnerPid, "SIGKILL");
  }
  if (grandchildPid && isProcessAlive(grandchildPid)) {
    process.kill(grandchildPid, "SIGKILL");
  }
}
