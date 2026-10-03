import { EventEmitter } from "node:events";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, delimiter, join } from "node:path";
import {
  MAX_TIMER_TIMEOUT_MS,
  MAX_TIMER_TIMEOUT_SECONDS,
} from "@openclaw/normalization-core/number-coercion";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  extractLastOpenClawVersionFromLog,
  resolveLatestVersion,
  withProgressOnStderr,
} from "../../scripts/e2e/parallels/common.ts";
import { testing as hostServerTesting } from "../../scripts/e2e/parallels/host-server.ts";
import { testing as packageArtifactTesting } from "../../scripts/e2e/parallels/package-artifact.ts";
import { PhaseRunner } from "../../scripts/e2e/parallels/phase-runner.ts";
import { spawnNodeEvalSync } from "../../src/test-utils/node-process.js";
import { acquireTestPortBlock } from "../../src/test-utils/port-claims.js";
import { useAutoCleanupTempDirTracker } from "../helpers/temp-dir.js";
import { TS_PATHS, TS_SOURCE, OS_TS_PATHS } from "./parallels-smoke-source.test-support.js";

const {
  guestTransports: transports,
  hostServer,
  linux,
  macos,
  npmUpdate,
  packageArtifact,
  parallelsVm,
  phaseRunner,
  snapshots,
  smokeCommon,
  windows,
} = TS_SOURCE;

const tempDirs = useAutoCleanupTempDirTracker(afterEach);

class FakeHostServerChild extends EventEmitter {
  exitCode: number | null = null;
  signalCode: NodeJS.Signals | null = null;
  readonly signals: string[] = [];

  kill(signal?: NodeJS.Signals | number): boolean {
    this.signals.push(String(signal));
    return true;
  }

  exit(): void {
    this.exitCode = 0;
    this.emit("exit", 0, null);
  }

  exitWithSignal(signal: NodeJS.Signals): void {
    this.signalCode = signal;
    this.emit("exit", null, signal);
  }
}

async function runFailingHostServer(fakePythonSource: string) {
  const tempDir = tempDirs.make("openclaw-parallels-host-server-");
  const fakePython = join(tempDir, "python3");
  writeFileSync(fakePython, fakePythonSource);
  chmodSync(fakePython, 0o755);
  const claim = await acquireTestPortBlock({ offsets: [0] });
  try {
    return spawnNodeEvalSync(
      `import { startHostServer } from "./${TS_PATHS.hostServer}"; await startHostServer({ dir: ".", hostIp: "127.0.0.1", port: ${claim.port}, label: "artifact" });`,
      {
        env: { ...process.env, PATH: `${tempDir}${delimiter}${process.env.PATH ?? ""}` },
        imports: ["tsx"],
        maxBuffer: 1024 * 1024,
      },
    );
  } finally {
    await claim.release();
  }
}

describe("Parallels smoke artifacts", () => {
  it("extracts the last OpenClaw version from a bounded log tail", async () => {
    const tempDir = tempDirs.make("openclaw-parallels-log-tail-");
    const logPath = join(tempDir, "phase.log");
    writeFileSync(logPath, ["OpenClaw 0.0.1", "x".repeat(4 * 1024 * 1024)].join("\n"));
    await expect(extractLastOpenClawVersionFromLog(logPath)).resolves.toBe("");

    writeFileSync(logPath, "\nOpenClaw 2026.6.6\nOpenClaw 2026.6.7", { flag: "a" });
    await expect(extractLastOpenClawVersionFromLog(logPath)).resolves.toBe("2026.6.7");
  });

  it("keeps snapshot, host, package, and quote helpers shared", () => {
    const common = TS_SOURCE.common;

    const packageArtifactExports = new Set(
      (common.match(/export \{([^}]*)\} from "\.\/package-artifact\.ts";/)?.[1] ?? "")
        .split(",")
        .map((name) => name.trim())
        .filter(Boolean),
    );
    expect(packageArtifactExports).toContain("packOpenClaw");
    expect(packageArtifactExports).toContain("resolveOpenClawRegistryVersion");
    expect(common).not.toContain('export * from "./package-artifact.ts"');
    expect(packageArtifact).toContain("withPackageLock");
    expect(packageArtifact).toContain("Wait for Parallels package lock");
    expect(packageArtifact).toContain('"--allow-unreleased-changelog"');
    expect(packageArtifact).toContain("filename !== path.basename(filename)");
    expect(packageArtifact).toContain("filename !== path.win32.basename(filename)");
    expect(packageArtifact).toContain("npm pack did not report a safe tarball filename");
    expect(packageArtifact).not.toContain("path.basename(packed)");
    expect(hostServer).toContain("hostUrl: `http://127.0.0.1:${port}`");
    expect(hostServer).toContain('OPENCLAW_NPM_REGISTRY_UPSTREAM: "https://registry.npmjs.org"');
    expect(hostServer).toContain("http.server");
    expect(smokeCommon).toContain("runSmokeLane");
    expect(smokeCommon).toContain("abstract class SmokeRunController");

    for (const scriptPath of OS_TS_PATHS) {
      const script = readFileSync(scriptPath, "utf8");

      expect(script, scriptPath).toContain("resolveSnapshot");
      expect(script, scriptPath).toContain("SmokeRunController");
      expect(script, scriptPath).not.toContain("def aliases(name: str)");
    }
  });

  it("bounds host artifact server startup stderr", () => {
    const retained = hostServerTesting.appendBoundedOutput(
      "a".repeat(10),
      Buffer.from("b".repeat(10)),
      12,
    );
    expect(retained).toBe(`${"a".repeat(2)}${"b".repeat(10)}`);
  });

  it("accepts npm 10/11 array and npm 12 workspace result shapes", () => {
    expect(
      packageArtifactTesting.resolveNpmPackTarballFilename([
        { filename: "openclaw-2026.6.11.tgz" },
      ]),
    ).toBe("openclaw-2026.6.11.tgz");
    expect(
      packageArtifactTesting.resolveNpmPackTarballFilename({
        openclaw: { filename: "openclaw-2026.6.11.tgz" },
      }),
    ).toBe("openclaw-2026.6.11.tgz");
  });

  it("keeps fresh package locks with malformed owner pids", async () => {
    const lockDir = tempDirs.make("openclaw-parallels-package-lock-");
    mkdirSync(lockDir, { recursive: true });
    writeFileSync(join(lockDir, "owner.json"), '{"pid":-1,"token":"stale"}\n');

    await expect(packageArtifactTesting.readLockOwner(lockDir)).resolves.toEqual({
      pid: undefined,
      token: "stale",
    });

    await packageArtifactTesting.removeStalePackageLock(lockDir, 2 * 60 * 60_000);

    expect(existsSync(lockDir)).toBe(true);
  });

  it("reclaims stale package locks with malformed owner pids", async () => {
    const lockDir = tempDirs.make("openclaw-parallels-package-lock-");
    mkdirSync(lockDir, { recursive: true });
    writeFileSync(join(lockDir, "owner.json"), '{"pid":-1,"token":"stale"}\n');

    await packageArtifactTesting.removeStalePackageLock(lockDir, 0);

    expect(existsSync(lockDir)).toBe(false);
  });

  it("removes a just-created package lock when owner writing fails", async () => {
    const parentDir = tempDirs.make("openclaw-parallels-package-lock-parent-");
    const lockDir = join(parentDir, "package.lock");
    const error = new Error("failed to write owner");

    await expect(
      packageArtifactTesting.acquirePackageLock(lockDir, "owner-token", {
        writeOwner: async () => {
          throw error;
        },
      }),
    ).rejects.toBe(error);

    expect(existsSync(lockDir)).toBe(false);
  });

  it("keeps JSON-mode progress off stdout", async () => {
    const stdoutWrite = vi.spyOn(process.stdout, "write").mockImplementation(() => true);
    const stderrWrite = vi.spyOn(process.stderr, "write").mockImplementation(() => true);
    try {
      await withProgressOnStderr(async () => {
        const { say } = await import("../../scripts/e2e/parallels/common.ts");
        say("progress");
        process.stdout.write('{"ok":true}\n');
      });

      expect(stdoutWrite).toHaveBeenCalledTimes(1);
      expect(stdoutWrite).toHaveBeenCalledWith('{"ok":true}\n');
      expect(JSON.parse(String(stdoutWrite.mock.calls[0]?.[0]))).toEqual({ ok: true });
      expect(stderrWrite).toHaveBeenCalledWith("==> progress\n");
    } finally {
      stdoutWrite.mockRestore();
      stderrWrite.mockRestore();
    }
  });

  it("waits for host artifact server exit after SIGKILL before stop resolves", async () => {
    vi.useFakeTimers();
    try {
      const child = new FakeHostServerChild();
      const stop = hostServerTesting.stopHostServerChild(child as never);
      expect(child.signals).toEqual(["SIGTERM"]);

      await vi.advanceTimersByTimeAsync(2_000);
      expect(child.signals).toEqual(["SIGTERM", "SIGKILL"]);

      let resolved = false;
      void stop.then(() => {
        resolved = true;
      });
      await Promise.resolve();
      expect(resolved).toBe(false);

      child.exit();
      await expect(stop).resolves.toBe(true);
      expect(resolved).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("treats signaled host artifact server children as already exited", async () => {
    const child = new FakeHostServerChild();
    child.exitWithSignal("SIGTERM");

    await expect(hostServerTesting.stopHostServerChild(child as never)).resolves.toBe(true);
    expect(child.signals).toEqual([]);
  });

  it("uses a temporary npmrc file and cleans it after resolving the latest package version", () => {
    const tempRoot = tempDirs.make("openclaw-parallels-version-");
    let userConfigPath = "";
    const version = resolveLatestVersion("", {
      createTempDir: (prefix) => {
        expect(prefix).toBe(join(tmpdir(), "openclaw-npm-"));
        return mkdtempSync(join(tempRoot, "npm-"));
      },
      runCommand: (command, args, options) => {
        userConfigPath = args.at(-1) ?? "";
        expect(command).toBe("npm");
        expect(args).toEqual(["view", "openclaw", "version", "--userconfig", userConfigPath]);
        expect(options).toEqual({ quiet: true });
        expect(statSync(userConfigPath).isFile()).toBe(true);
        return { status: 0, stderr: "", stdout: "2026.6.1\n" };
      },
    });

    expect(version).toBe("2026.6.1");
    expect(basename(userConfigPath)).toBe("npmrc");
    expect(existsSync(userConfigPath)).toBe(false);
  });

  it.runIf(process.platform !== "win32")(
    "reports only the bounded host artifact server stderr tail",
    async () => {
      const result = await runFailingHostServer(
        `#!/usr/bin/env bash
set -euo pipefail; printf 'BEGIN_MARKER\\n' >&2; head -c 50000 </dev/zero | tr '\\0' x >&2; printf '\\nTAIL_MARKER\\n' >&2; head -c 30000 </dev/zero | tr '\\0' x >&2; exit 42`,
      );

      expect(result.status).toBe(1);
      expect(result.stderr).toContain("host artifact server exited early");
      expect(result.stderr).toContain("TAIL_MARKER");
      expect(result.stderr).not.toContain("BEGIN_MARKER");
      expect(Buffer.byteLength(result.stderr, "utf8")).toBeLessThan(90 * 1024);
    },
  );

  it.runIf(process.platform !== "win32")(
    "reports signaled host artifact server startup exits immediately",
    async () => {
      const result = await runFailingHostServer(
        `#!/usr/bin/env bash
kill -TERM "$$"`,
      );

      expect(result.status).toBe(1);
      expect(result.stderr).toContain("host artifact server exited early: signal SIGTERM");
      expect(result.stderr).not.toContain("did not start");
    },
  );

  it("clears phase timers and applies phase deadlines to guest commands", () => {
    expect(phaseRunner).toContain("clearTimeout(timer)");
    expect(phaseRunner).toContain("remainingTimeoutMs");
    expect(transports).toContain("this.phases.remainingTimeoutMs");
    expect(parallelsVm).toContain("PRLCTL_STATUS_TIMEOUT_MS");
    expect(parallelsVm).toContain("probeTimeoutMs");
    expect(snapshots).toContain("SNAPSHOT_LIST_TIMEOUT_MS");

    for (const scriptPath of OS_TS_PATHS) {
      const script = readFileSync(scriptPath, "utf8");

      expect(script, scriptPath).toContain("PhaseRunner");
      expect(script, scriptPath).toContain("validateSnapshotRestoreMode(this.options.mode");
      expect(script, scriptPath).toContain("this.phases.remainingTimeoutMs");
      expect(script, scriptPath).toContain("timeoutMs:");
    }

    expect(macos).toContain("currentRunningSnapshotInfo(this.options.vmName)");
    expect(macos).toContain("shouldSkipSnapshotRestore()");
    expect(macos).toContain("Skip snapshot restore; using current running VM");

    expect(linux).toContain("probeTimeoutMs: () => this.phases.remainingTimeoutMs(30_000)");
    expect(windows).toContain("probeTimeoutMs: () => this.phases.remainingTimeoutMs(30_000)");
    expect(macos).toContain("probeTimeoutMs: () => this.phases.remainingTimeoutMs(30_000)");
    expect(macos).toContain("timeoutMs: this.phases.remainingTimeoutMs(360_000)");
  });

  it("streams full phase logs to disk while bounding the failure tail", async () => {
    const runDir = tempDirs.make("openclaw-parallels-phase-");
    const logPhaseRunner = new PhaseRunner(runDir, 128);
    const writes: string[] = [];
    const stderrWrite = vi.spyOn(process.stderr, "write").mockImplementation((chunk) => {
      writes.push(String(chunk));
      return true;
    });

    try {
      await expect(
        logPhaseRunner.phase("noisy", 30, () => {
          logPhaseRunner.append(`old-${"x".repeat(256)}`);
          logPhaseRunner.append("recent failure");
          throw new Error("phase failed");
        }),
      ).rejects.toThrow("phase failed");

      const logText = readFileSync(join(runDir, "noisy.log"), "utf8");
      expect(logText).toContain("old-");
      expect(logText).toContain("recent failure");
      const stderr = writes.join("");
      expect(stderr).toContain("phase log tail truncated");
      expect(stderr).toContain("recent failure");
      expect(stderr).not.toContain(`old-${"x".repeat(200)}`);
    } finally {
      stderrWrite.mockRestore();
    }
  });

  it("clamps oversized phase timers before scheduling", async () => {
    const runDir = tempDirs.make("openclaw-parallels-phase-timeout-");
    const timerPhaseRunner = new PhaseRunner(runDir, 128);
    const setTimeoutSpy = vi.spyOn(globalThis, "setTimeout");

    try {
      await expect(
        timerPhaseRunner.phase("oversized", MAX_TIMER_TIMEOUT_SECONDS + 1, () => undefined),
      ).resolves.toBeUndefined();
      expect(setTimeoutSpy).toHaveBeenCalledWith(expect.any(Function), MAX_TIMER_TIMEOUT_MS);
      expect(readFileSync(join(runDir, "phase-timings.json"), "utf8")).toContain(
        `"timeoutSeconds": ${MAX_TIMER_TIMEOUT_SECONDS + 1}`,
      );
    } finally {
      setTimeoutSpy.mockRestore();
    }
  });

  it("writes Parallels phase timing artifacts", () => {
    expect(phaseRunner).toContain("phase-timings.json");
    expect(phaseRunner).toContain("slowest");
    expect(npmUpdate).toContain("timings: this.timings");
    expect(npmUpdate).toContain("recordTiming");
  });
});
