import { ChildProcess, spawn } from "node:child_process";
import { join } from "node:path";
import { PassThrough } from "node:stream";
import { afterEach, describe, expect, it, vi } from "vitest";
import { repoRoot } from "../../scripts/e2e/parallels/common.ts";
import { NpmUpdateSmoke, parseArgs } from "../../scripts/e2e/parallels/npm-update-smoke.ts";
import { assertDevChannelUpdate } from "../../scripts/e2e/parallels/smoke-common.ts";
import { withEnv } from "../../src/test-utils/env.js";
import { useAutoCleanupTempDirTracker } from "../helpers/temp-dir.js";
import { TS_SOURCE } from "./parallels-smoke-source.test-support.js";

vi.mock("node:child_process", async (importOriginal) => {
  const original = await importOriginal<typeof import("node:child_process")>();
  return { ...original, spawn: vi.fn() };
});

const { npmUpdate, npmUpdateScripts } = TS_SOURCE;
const tempDirs = useAutoCleanupTempDirTracker(afterEach);

describe("Parallels smoke npm update", () => {
  it("retries failed aggregate fresh lanes once from a restored snapshot", () => {
    expect(npmUpdate).toContain("retrying once from restored snapshot");
    expect(npmUpdate).toContain('attempt === 1 ? "" : `-retry-${attempt}`');
    expect(npmUpdate).toContain("failed after retry");
  });

  it("passes aggregate model overrides into each OS fresh lane", () => {
    expect(npmUpdate).toContain("scripts/e2e/parallels/${platform}-smoke.ts");
    expect(npmUpdate).toContain('"--import",\n      "tsx",\n      script,');
    expect(npmUpdate).toContain("this.formatRerun(process.execPath, args, commandEnv)");
    expect(npmUpdate).toContain('"--model"');
    expect(npmUpdate).toContain("auth.modelId");
    expect(npmUpdate).toContain("authForPlatform");
    expect(npmUpdate).toContain("OPENCLAW_PARALLELS_LINUX_DISABLE_BONJOUR");
  });

  it.each([
    ["linux", "fresh"],
    ["macos", "fresh"],
    ["windows", "fresh"],
    ["linux", "fresh-target"],
    ["macos", "fresh-target"],
    ["windows", "fresh-target"],
  ] as const)("launches %s %s directly with its preserved contract", async (platform, phase) => {
    const options = parseArgs([
      "--provider",
      "openai",
      "--model",
      "openai/fixture-model",
      "--api-key-env",
      "OPENCLAW_PARALLELS_TEST_API_KEY",
      "--package-spec",
      "openclaw@2026.1.1",
      "--json",
    ]);
    const smoke = withEnv(
      { OPENCLAW_PARALLELS_TEST_API_KEY: "fixture" },
      () => new NpmUpdateSmoke(options),
    );
    smoke["runDir"] = tempDirs.make("openclaw-parallels-fresh-invocation-");
    smoke["targetRegistryUrl"] = "http://192.0.2.2:48123";
    smoke["targetRegistryHostUrl"] = "http://127.0.0.1:48123";
    const extraArgs = ["--vm", `${platform} fixture VM`];
    if (platform === "macos") {
      extraArgs.push("--snapshot-hint", "fixture snapshot");
    }
    const env = {
      CUSTOM_FRESH_ENV: "preserved",
      ...(platform === "linux" ? { OPENCLAW_PARALLELS_LINUX_DISABLE_BONJOUR: "1" } : {}),
    };
    const child = new ChildProcess();
    child.stdout = new PassThrough();
    child.stderr = new PassThrough();
    vi.mocked(spawn).mockClear().mockReturnValueOnce(child);
    const job = smoke["spawnFresh"](
      "fixture",
      platform,
      extraArgs,
      env,
      options.packageSpec,
      phase,
    );
    try {
      expect(spawn).toHaveBeenCalledOnce();
      expect(spawn).toHaveBeenCalledWith(
        process.execPath,
        [
          "--import",
          "tsx",
          `scripts/e2e/parallels/${platform}-smoke.ts`,
          "--mode",
          "fresh",
          "--provider",
          "openai",
          "--model",
          "openai/fixture-model",
          "--api-key-env",
          "OPENCLAW_PARALLELS_TEST_API_KEY",
          "--target-package-spec",
          options.packageSpec,
          ...(phase === "fresh-target" ? ["--npm-registry", "http://192.0.2.2:48123"] : []),
          "--json",
          ...extraArgs,
        ],
        {
          cwd: repoRoot,
          detached: process.platform !== "win32",
          env: {
            ...process.env,
            ...env,
            ...(phase === "fresh-target"
              ? {
                  NPM_CONFIG_REGISTRY: "http://127.0.0.1:48123",
                  npm_config_registry: "http://127.0.0.1:48123",
                }
              : {}),
          },
          stdio: ["ignore", "pipe", "pipe"],
        },
      );
      expect(job.logPath).toBe(join(smoke["runDir"], `${platform}-${phase}.log`));
      expect(job.rerunCommand).toContain(`scripts/e2e/parallels/${platform}-smoke.ts`);
      expect(job.rerunCommand).toContain(process.execPath);
    } finally {
      child.exitCode = 0;
      child.emit("close", 0);
      child.stdout.destroy();
      child.stderr.destroy();
      await expect(job.promise).resolves.toBe(0);
    }
  });

  it("keeps the Windows update config scrub compatible with PowerShell 5.1", () => {
    expect(npmUpdateScripts).not.toContain("ConvertFrom-Json -AsHashtable");
    expect(npmUpdateScripts).toContain(
      "$nodeScript | Set-Content -Path $nodeScriptPath -Encoding UTF8",
    );
  });

  it("keeps aggregate update guest scripts isolated from the npm-update orchestrator", () => {
    const orchestrator = npmUpdate;
    const updateScripts = npmUpdateScripts;

    expect(orchestrator).toContain("macosUpdateScript");
    expect(orchestrator).toContain("windowsUpdateScript");
    expect(orchestrator).toContain("linuxUpdateScript");
    expect(orchestrator).toContain('import { randomUUID } from "node:crypto"');
    expect(orchestrator).not.toContain("process.pid}-${Date.now()");
    expect(orchestrator).not.toContain("Remove-FuturePluginEntries");
    expect(updateScripts).toContain("Remove-FuturePluginEntries");
    expect(updateScripts).toContain("scrub_future_plugin_entries");
    expect(updateScripts).toContain("Invoke-OpenClaw update");
    expect(updateScripts).toContain("Parallels npm update smoke test assistant.");
  });

  it.each([
    {
      name: "unpinned main",
      status: '"installKind": "git", "value": "dev", "branch": "main"',
      target: undefined,
      head: "",
      reads: 0,
    },
    {
      name: "empty target",
      status: '"installKind": "git", "value": "dev", "branch": "main"',
      target: "",
      head: "",
      reads: 0,
    },
    {
      name: "pinned HEAD with banner and CRLF",
      status: '"installKind": "git", "value": "dev", "branch": "HEAD"',
      target: "a".repeat(40),
      head: `checkout banner\r\n${"a".repeat(40)}\r\n`,
      reads: 1,
    },
    {
      name: "missing install kind before all other checks",
      status: "",
      target: "a".repeat(40),
      head: "",
      reads: 0,
      error: 'dev update status missing "installKind": "git"',
    },
    {
      name: "missing dev channel before branch and checkout",
      status: '"installKind": "git"',
      target: "a".repeat(40),
      head: "",
      reads: 0,
      error: 'dev update status missing "value": "dev"',
    },
    {
      name: "missing pinned branch before checkout",
      status: '"installKind": "git", "value": "dev"',
      target: "a".repeat(40),
      head: "",
      reads: 0,
      error: 'dev update status missing "branch": "HEAD"',
    },
    {
      name: "wrong unpinned branch",
      status: '"installKind": "git", "value": "dev", "branch": "HEAD"',
      target: undefined,
      head: "",
      reads: 0,
      error: 'dev update status missing "branch": "main"',
    },
    {
      name: "empty pinned checkout",
      status: '"installKind": "git", "value": "dev", "branch": "HEAD"',
      target: "a".repeat(40),
      head: "\r\n",
      reads: 1,
      error: `dev update checkout head <empty> did not match ${"a".repeat(40)}`,
    },
    {
      name: "mismatching pinned checkout",
      status: '"installKind": "git", "value": "dev", "branch": "HEAD"',
      target: "a".repeat(40),
      head: "b".repeat(40),
      reads: 1,
      error: `dev update checkout head ${"b".repeat(40)} did not match ${"a".repeat(40)}`,
    },
  ])("validates dev updates: $name", ({ status, target, head, reads, error }) => {
    const readCheckoutHead = vi.fn(() => head);
    const verify = () => assertDevChannelUpdate(status, target, readCheckoutHead);
    if (error) {
      expect(verify).toThrow(new Error(error));
    } else {
      expect(verify).not.toThrow();
    }
    expect(readCheckoutHead).toHaveBeenCalledTimes(reads);
  });

  it("preserves a dev checkout reader failure by identity", () => {
    const failure = new Error("checkout command failed");
    const readCheckoutHead = vi.fn(() => {
      throw failure;
    });
    let caught: unknown;
    try {
      assertDevChannelUpdate(
        '"installKind": "git", "value": "dev", "branch": "HEAD"',
        "a".repeat(40),
        readCheckoutHead,
      );
    } catch (error) {
      caught = error;
    }
    expect(caught).toBe(failure);
    expect(readCheckoutHead).toHaveBeenCalledOnce();
  });
});
