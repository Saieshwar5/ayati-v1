import { existsSync, readFileSync } from "node:fs";
import { join, win32 } from "node:path";
import { MAX_TIMER_TIMEOUT_MS } from "@openclaw/normalization-core/number-coercion";
import { describe, expect, it } from "vitest";
import { run } from "../../scripts/e2e/parallels/common.ts";
import { resolveHostCommandInvocation } from "../../scripts/e2e/parallels/host-command.ts";
import { withinTest } from "../helpers/promise.js";
import { testNodeExecPath } from "./parallels-smoke-command.test-support.js";
import {
  tempDirs,
  isProcessAlive,
  waitForProcessGone,
  runNode,
  drainableProcessTreeScript,
  writeDrainDeadlinePreload,
  SIGNAL_PARENT_SCRIPT,
  createSignaledHostCommandFixture,
  forceKillSignaledFixture,
} from "./parallels-smoke-host-command.test-support.js";

describe("Parallels smoke host command", () => {
  it("returns timed-out host command status when check is disabled", () => {
    const result = runNode("process.stdout.write('partial'); setTimeout(() => {}, 1000);", {
      check: false,
      timeoutMs: 50,
    });

    expect(result.status).toBe(124);
    expect(result.stdout).toBeTypeOf("string");
  });

  it("clamps oversized timed host command wrapper timeouts", () => {
    const result = runNode("setTimeout(() => process.exit(0), 25);", {
      check: false,
      timeoutMs: MAX_TIMER_TIMEOUT_MS + 1,
    });

    expect(result.status).toBe(0);
  });

  it.runIf(process.platform !== "win32")(
    "lets timed host command descendants drain before force kill",
    () => {
      const tempDir = tempDirs.make("openclaw-parallels-host-command-drain-");
      const readyFile = join(tempDir, "ready");
      const drainFile = join(tempDir, "drained");
      const deadlineFile = join(tempDir, "deadline");
      const preload = writeDrainDeadlinePreload(tempDir);

      // Force descendant startup past the deadline, then release the real timeout
      // callback only after signal handlers are installed. Cleanup timers stay real.
      const result = runNode(drainableProcessTreeScript(25), {
        check: false,
        env: {
          ...process.env,
          DRAIN_FILE: drainFile,
          READY_FILE: readyFile,
          DEADLINE_FILE: deadlineFile,
          NODE_OPTIONS: `${process.env.NODE_OPTIONS ?? ""} --require ${JSON.stringify(preload)}`,
        },
        timeoutMs: 100,
      });

      expect(result.status).toBe(124);
      expect(readFileSync(deadlineFile, "utf8")).toBe("elapsed");
      expect(existsSync(readyFile)).toBe(true);
      expect(readFileSync(drainFile, "utf8")).toBe("drained");
    },
  );

  it.runIf(process.platform !== "win32")("throws checked timed host command timeouts", () => {
    expect(() => runNode("setInterval(() => {}, 1000);", { timeoutMs: 50 })).toThrow(
      /timed out after 50ms/u,
    );
  });

  it.runIf(process.platform !== "win32")("preserves child exit 124 in timed host commands", () => {
    const result = runNode("process.exit(124)", {
      check: false,
      timeoutMs: 1_000,
    });

    expect(result.status).toBe(124);
  });

  it.runIf(process.platform !== "win32")(
    "kills timed-out host command process groups",
    async ({ signal }) => {
      const tempDir = tempDirs.make("openclaw-parallels-host-command-");
      const grandchildPidPath = join(tempDir, "grandchild.pid");
      const deadlineFile = join(tempDir, "deadline");
      const preload = writeDrainDeadlinePreload(tempDir, 200);
      let grandchildPid = 0;

      try {
        const result = runNode(SIGNAL_PARENT_SCRIPT, {
          check: false,
          env: {
            ...process.env,
            OPENCLAW_TEST_GRANDCHILD_PID: grandchildPidPath,
            OPENCLAW_TEST_READY_FILE: join(tempDir, "ready"),
            READY_FILE: grandchildPidPath,
            DEADLINE_FILE: deadlineFile,
            NODE_OPTIONS: `${process.env.NODE_OPTIONS ?? ""} --require ${JSON.stringify(preload)}`,
          },
          timeoutMs: 200,
        });

        expect(result.status).toBe(124);
        expect(readFileSync(deadlineFile, "utf8")).toBe("elapsed");
        grandchildPid = Number.parseInt(readFileSync(grandchildPidPath, "utf8"), 10);
        expect(Number.isInteger(grandchildPid)).toBe(true);
        await waitForProcessGone(grandchildPid, signal);
      } finally {
        if (grandchildPid && isProcessAlive(grandchildPid)) {
          process.kill(grandchildPid, "SIGKILL");
        }
      }
    },
  );

  it.runIf(process.platform !== "win32")(
    "settles timed host commands when an escaped descendant retains child pipes",
    () => {
      const tempDir = tempDirs.make("openclaw-parallels-host-command-pipes-");
      const grandchildPidPath = join(tempDir, "grandchild.pid");
      const deadlineFile = join(tempDir, "deadline");
      const preload = writeDrainDeadlinePreload(tempDir, 200);
      let grandchildPid = 0;
      // Outlive the assertion bound, but self-clean if PID setup fails.
      const grandchildScript = "setTimeout(() => process.exit(0), 3_000);";
      const parentScript = [
        "const { spawn } = require('node:child_process');",
        "const { renameSync, writeFileSync } = require('node:fs');",
        `const child = spawn(process.execPath, ['-e', ${JSON.stringify(grandchildScript)}], {`,
        "  detached: true,",
        "  env: process.env,",
        "  stdio: ['ignore', 'inherit', 'inherit'],",
        "});",
        "const pidPath = process.env.GRANDCHILD_PID_PATH;",
        "writeFileSync(pidPath + '.tmp', String(child.pid));",
        "renameSync(pidPath + '.tmp', pidPath);",
        "child.unref();",
        "setInterval(() => {}, 1000);",
      ].join("\n");
      const startedAt = Date.now();

      try {
        const result = run(testNodeExecPath, ["-e", parentScript], {
          check: false,
          env: {
            ...process.env,
            GRANDCHILD_PID_PATH: grandchildPidPath,
            READY_FILE: grandchildPidPath,
            DEADLINE_FILE: deadlineFile,
            NODE_OPTIONS: `${process.env.NODE_OPTIONS ?? ""} --require ${JSON.stringify(preload)}`,
          },
          quiet: true,
          // Let the command spawn its pipe holder before exercising timeout settlement.
          timeoutMs: 200,
        });

        const durationMs = Date.now() - startedAt;
        // The renamed path publishes a complete PID even if timeout settles first.
        expect(existsSync(grandchildPidPath)).toBe(true);
        grandchildPid = Number(readFileSync(grandchildPidPath, "utf8"));

        expect(Number.isInteger(grandchildPid)).toBe(true);
        expect(grandchildPid).toBeGreaterThan(1);
        expect(result.status).toBe(124);
        expect(readFileSync(deadlineFile, "utf8")).toBe("elapsed");
        expect(durationMs).toBeLessThan(2_000);
      } finally {
        if (Number.isInteger(grandchildPid) && grandchildPid > 1 && isProcessAlive(grandchildPid)) {
          process.kill(-grandchildPid, "SIGKILL");
        }
      }
    },
  );

  it.runIf(process.platform !== "win32")(
    "reaps externally signaled timed host command descendants",
    async ({ signal }) => {
      const fixture = createSignaledHostCommandFixture();
      let runnerPid = 0;
      let grandchildPid = 0;

      try {
        runnerPid = fixture.runner.pid ?? 0;
        expect(runnerPid).toBeGreaterThan(0);
        await withinTest(fixture.ready, signal);
        grandchildPid = Number.parseInt(readFileSync(fixture.grandchildPidPath, "utf8"), 10);

        process.kill(-runnerPid, "SIGTERM");

        await expect(withinTest(fixture.closed, signal)).resolves.toEqual({
          code: null,
          signal: "SIGTERM",
        });
        await waitForProcessGone(grandchildPid, signal);
      } finally {
        if (!grandchildPid && existsSync(fixture.grandchildPidPath)) {
          grandchildPid = Number.parseInt(readFileSync(fixture.grandchildPidPath, "utf8"), 10);
        }
        forceKillSignaledFixture(runnerPid, grandchildPid, true);
        if (existsSync(fixture.parentPidPath)) {
          const parentPid = Number.parseInt(readFileSync(fixture.parentPidPath, "utf8"), 10);
          if (isProcessAlive(parentPid)) {
            process.kill(-parentPid, "SIGKILL");
          }
        }
        await fixture.closed;
      }
    },
  );

  it.runIf(process.platform !== "win32")("preserves timed host command spawn errors", () => {
    expect(() =>
      run("openclaw-definitely-missing-host-command", [], {
        check: false,
        quiet: true,
        timeoutMs: 50,
      }),
    ).toThrow(/ENOENT/u);
  });

  it.runIf(process.platform !== "win32")(
    "does not treat timed command stderr as wrapper control data",
    () => {
      const result = runNode("process.stderr.write('__OPENCLAW_HOST_COMMAND_SPAWN_ERROR__{}\\n')", {
        check: false,
        timeoutMs: 500,
      });

      expect(result.status).toBe(0);
    },
  );

  it.runIf(process.platform !== "win32")("preserves timed host command output capture", () => {
    const expected = "x".repeat(256 * 1024);
    const result = runNode("process.stdout.write('x'.repeat(256 * 1024))", {
      check: false,
      timeoutMs: 1_000,
    });

    expect(result.status).toBe(0);
    expect(result.stdout).toBe(expected);
  });

  it.runIf(process.platform !== "win32")(
    "ignores broken stdin pipes from timed host commands that exit early",
    () => {
      const result = runNode("process.exit(0)", {
        check: false,
        input: "x".repeat(1024 * 1024),
        timeoutMs: 1_000,
      });

      expect(result.status).toBe(0);
    },
  );

  it("routes Windows host pnpm and npm shims through safe runners", () => {
    const comSpec = "C:\\Windows\\System32\\cmd.exe";

    expect(
      resolveHostCommandInvocation("pnpm", ["build"], {
        env: {
          ComSpec: comSpec,
          npm_execpath: "C:\\Tools\\pnpm.cmd",
        },
        platform: "win32",
      }),
    ).toEqual({
      args: ["/d", "/s", "/c", "C:\\Tools\\pnpm.cmd build"],
      command: comSpec,
      shell: false,
      windowsVerbatimArguments: true,
    });

    const execPath = "C:\\nodejs\\node.exe";
    const npmCmdPath = win32.resolve(win32.dirname(execPath), "npm.cmd");
    expect(
      resolveHostCommandInvocation("npm", ["view", "openclaw", "version"], {
        env: { ComSpec: comSpec },
        execPath,
        existsSync: (candidate) => candidate === npmCmdPath,
        platform: "win32",
      }),
    ).toEqual({
      args: ["/d", "/s", "/c", `${npmCmdPath} view openclaw version`],
      command: comSpec,
      shell: false,
      windowsVerbatimArguments: true,
    });
  });

  it("wraps explicit Windows batch host commands without shell mode", () => {
    expect(
      resolveHostCommandInvocation("C:\\Tools\\helper.cmd", ["@scope/pkg@^1.0.0"], {
        comSpec: "cmd.exe",
        platform: "win32",
      }),
    ).toEqual({
      args: ["/d", "/s", "/c", "C:\\Tools\\helper.cmd @scope/pkg@^^1.0.0"],
      command: "cmd.exe",
      shell: false,
      windowsVerbatimArguments: true,
    });
  });

  it("ignores ambient ComSpec for Windows host batch commands", () => {
    expect(
      resolveHostCommandInvocation("C:\\Tools\\helper.cmd", ["build"], {
        env: {
          ComSpec: "C:\\Users\\test\\bin\\cmd.exe",
          SystemRoot: "D:\\Windows",
        },
        platform: "win32",
      }).command,
    ).toBe("D:\\Windows\\System32\\cmd.exe");
  });
});
