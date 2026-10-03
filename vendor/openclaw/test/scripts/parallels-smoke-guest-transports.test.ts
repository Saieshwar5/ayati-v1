import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  LinuxGuest,
  MacosGuest,
  WindowsGuest,
  runWindowsBackgroundPowerShell,
} from "../../scripts/e2e/parallels/guest-transports.ts";
import { PhaseRunner } from "../../scripts/e2e/parallels/phase-runner.ts";
import { withEnv } from "../../src/test-utils/env.js";
import { useAutoCleanupTempDirTracker } from "../helpers/temp-dir.js";
import { fakeResult, runFakePosixBackground } from "./parallels-smoke-background.test-support.js";
import { fakePrlctlEnv, writeNodeFakePrlctl } from "./parallels-smoke-command.test-support.js";
import { TS_SOURCE } from "./parallels-smoke-source.test-support.js";

const { guestTransports: transports, macos, windows } = TS_SOURCE;

const tempDirs = useAutoCleanupTempDirTracker(afterEach);

class ExhaustedCleanupPhaseRunner {
  output = "";
  remainingTimeoutCalls = 0;

  append(text: string): void {
    this.output += text;
  }

  remainingTimeoutMs(fallbackMs?: number): number {
    this.remainingTimeoutCalls += 1;
    if (this.remainingTimeoutCalls > 2) {
      throw new Error("phase deadline exceeded before starting guest command");
    }
    return fallbackMs ?? 30_000;
  }
}

function createMacosGuest(phases: PhaseRunner): MacosGuest {
  return new MacosGuest(
    {
      getTransport: () => "current-user",
      getUser: () => "runner",
      path: "/usr/bin:/bin",
      resolveDesktopHome: () => "/Users/runner",
      vmName: "macOS VM",
    },
    phases,
  );
}

describe("Parallels smoke guest transports", () => {
  it("cleans POSIX guest scripts after the phase deadline is exhausted", () => {
    const tempDir = tempDirs.make("openclaw-parallels-posix-cleanup-");
    const logPath = join(tempDir, "prlctl.log");
    writeNodeFakePrlctl(
      tempDir,
      `const fs = process.getBuiltinModule("node:fs"); const command = \` \${args.join(" ")} \`; fs.appendFileSync(${JSON.stringify(logPath)}, args.join(" ") + "\\n"); if (command.includes(" dd of=/tmp/openclaw-parallels-") || command.includes(" /bin/dd of=/tmp/openclaw-parallels-")) fs.readFileSync(0); if (command.includes(" bash /tmp/openclaw-parallels-") || command.includes(" /bin/bash /tmp/openclaw-parallels-")) process.exit(1); if (command.includes(" /bin/rm -f /tmp/openclaw-parallels-")) fs.appendFileSync(${JSON.stringify(logPath)}, "cleanup\\n"); process.exit(0);`,
    );

    withEnv(fakePrlctlEnv(tempDir), () => {
      const linuxPhases = new ExhaustedCleanupPhaseRunner();
      const cleanupLinux = new LinuxGuest("Linux VM", linuxPhases as unknown as PhaseRunner);

      expect(() => cleanupLinux.bash("echo linux")).toThrow(
        "Linux guest command failed with exit code 1",
      );
      expect(linuxPhases.remainingTimeoutCalls).toBe(2);

      const macosPhases = new ExhaustedCleanupPhaseRunner();
      const cleanupMacos = createMacosGuest(macosPhases as unknown as PhaseRunner);

      expect(() => cleanupMacos.sh("echo macos")).toThrow(
        "macOS guest command failed with exit code 1",
      );
      expect(macosPhases.remainingTimeoutCalls).toBe(2);
    });

    const log = readFileSync(logPath, "utf8");
    expect(log.match(/^cleanup$/gm)).toHaveLength(2);
  });

  it("rejects Parallels macOS guest session false-success output", () => {
    const tempDir = tempDirs.make("openclaw-parallels-session-unavailable-");
    writeNodeFakePrlctl(
      tempDir,
      `console.error("Unable to open new session in this virtual machine."); process.exit(0);`,
    );

    withEnv(fakePrlctlEnv(tempDir), () => {
      const phases = {
        append: () => undefined,
        remainingTimeoutMs: (fallbackMs?: number) => fallbackMs ?? 30_000,
      };
      const unavailableMacos = createMacosGuest(phases as unknown as PhaseRunner);

      expect(() => unavailableMacos.exec(["true"])).toThrow(
        "macOS guest command failed: Parallels guest session unavailable",
      );
    });
  });

  it("runs POSIX guest shell scripts with a normal install umask", () => {
    expect(transports.match(/umask 022/g)).toHaveLength(2);
  });

  it("uses collision-resistant guest script names", () => {
    expect(transports).toContain('import { randomUUID } from "node:crypto"');
    expect(transports).toContain("guestScriptName");
    expect(transports).not.toContain("Date.now()}.sh");
    expect(transports).not.toContain("Date.now()}.ps1");
    expect(transports).not.toContain("Math.random()");
  });

  it("runs Windows ref onboarding through a detached done-file runner", () => {
    expect(windows).toContain("guestPowerShellBackground");
    expect(windows).toContain("runWindowsBackgroundPowerShell");
    expect(transports).toContain("Join-Path (Join-Path $env:WINDIR 'Temp\\\\openclaw-parallels')");
    expect(transports).toContain("icacls.exe $runDir /inheritance:r");
    expect(transports).toContain("__OPENCLAW_BACKGROUND_DONE__");
    expect(transports).toContain("__OPENCLAW_BACKGROUND_EXIT__");
    expect(transports).toContain("poll.status !== 0 && poll.status !== 124");
    expect(transports).toContain('cmd.exe /d /s /c start "" /b powershell.exe');
    expect(transports).toContain('if exist "${windowsDonePath}"');
    expect(transports).toContain('type "%WINDIR%\\\\Temp\\\\${guestRunDir}\\\\run.log"');
    expect(transports).toContain("WINDOWS_BACKGROUND_LOG_MAX_BYTES");
    expect(transports).toContain("Write-OpenClawUtf8File $pidPath ([string]$PID)");
    expect(transports).toContain('launch.stdout.includes("started")');
    expect(transports).toContain("waitForWindowsBackgroundMaterialized");
  });

  it("runs the macOS dev update through a detached done-file runner", () => {
    expect(macos).toContain('this.guest.shBackground(\n      "macos-update-dev"');
    expect(transports).toContain('spawn("/bin/bash"');
    expect(transports).toContain("detached: true");
    expect(transports).toContain("child.unref()");
    expect(transports).toContain("POSIX_BACKGROUND_LOG_MAX_BYTES");
    expect(transports).toContain('runGuest(["/bin/test", "-f", donePath]');
    expect(transports).toContain('runGuest(["/bin/cat", exitPath]');
    expect(transports).toContain('["/bin/mkdir", "-m", "700", "-p", runDir]');
    expect(transports).toContain('command=$(/bin/ps -p "$background_pid" -o command=');
    expect(transports).toContain("*${posixSingleQuote(runnerPath)}*)");
    expect(transports).not.toContain('transport(["/bin/bash", "-c"');
    expect(macos).toContain(
      'fs.existsSync(configPath) ? JSON.parse(fs.readFileSync(configPath, "utf8")) : {}',
    );
    expect(macos).toContain("fs.mkdirSync(path.dirname(configPath), { recursive: true })");
  });

  it("accepts an ambiguous POSIX background launch after its run materializes", async () => {
    const output: string[] = [];
    const { result } = runFakePosixBackground(
      {
        exit: [fakeResult(124), fakeResult(0, "0\n")],
        launch: fakeResult(124),
        tail: () => fakeResult(0, "update complete\n"),
      },
      { append: (chunk) => output.push(String(chunk)), script: "echo update" },
    );
    await result;

    expect(output.join("")).toContain("update complete");
  });

  it("propagates a detached POSIX background exit failure", async () => {
    const { result } = runFakePosixBackground(
      { exit: [fakeResult(0, "7\n")], tail: () => fakeResult(0, "update failed\n") },
      { script: "exit 7" },
    );

    await expect(result).rejects.toThrow("macos update failed");
  });

  it("reads the POSIX background exit after log drain consumes the deadline", async () => {
    const { result, state } = runFakePosixBackground(
      {
        exit: [fakeResult(0, "0\n")],
        tail: () => {
          const until = Date.now() + 30;
          while (Date.now() < until) {
            // Simulate a completed log drain that exhausts the phase deadline.
          }
          return fakeResult(0, "update complete\n");
        },
      },
      { script: "echo update", timeoutMs: 25 },
    );
    await result;

    expect(state.exitRead).toBe(true);
  });

  it("cleans up an ambiguous POSIX launch when PID materialization is missed", async () => {
    const { result, state } = runFakePosixBackground(
      { launch: fakeResult(124), pid: fakeResult(1) },
      { script: "sleep 60", timeoutMs: 25 },
    );

    await expect(result).rejects.toThrow("macos update background launch failed");

    expect(state.cleanupRun).toBe(true);
  });

  it("force-stops the verified POSIX background process tree on timeout", async () => {
    const { result, state } = runFakePosixBackground(
      { done: fakeResult(1) },
      { script: "sleep 60", timeoutMs: 25 },
    );

    await expect(result).rejects.toThrow("macos update timed out");

    expect(state.cleanupPayload).toContain('command=$(/bin/ps -p "$background_pid" -o command=');
    expect(state.cleanupPayload).toContain('for child in $(/usr/bin/pgrep -P "$1"');
    expect(state.cleanupPayload).toContain('/bin/kill -TERM "$1"');
    expect(state.cleanupPayload).toContain('/bin/kill -KILL "$1"');
  });

  it("carries refreshed Windows guest environment into every PowerShell script", () => {
    const tempDir = tempDirs.make("openclaw-parallels-windows-env-");
    const scriptsPath = join(tempDir, "scripts.jsonl");
    writeNodeFakePrlctl(
      tempDir,
      `if (args.includes("-EncodedCommand")) { const fs = process.getBuiltinModule("node:fs"); fs.appendFileSync(${JSON.stringify(scriptsPath)}, JSON.stringify(fs.readFileSync(0, "utf8")) + "\\n"); } process.exit(0);`,
    );
    let registry = "http://192.0.2.2:48123/first";
    withEnv(fakePrlctlEnv(tempDir), () => {
      const guest = new WindowsGuest("Windows VM", new PhaseRunner(tempDir), () => ({
        NPM_CONFIG_REGISTRY: registry,
      }));
      guest.powershell("Write-Output $env:NPM_CONFIG_REGISTRY");
      registry = "http://192.0.2.2:48123/second's";
      guest.powershell("Write-Output $env:NPM_CONFIG_REGISTRY");
    });

    const scripts = readFileSync(scriptsPath, "utf8")
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line) as string);
    expect(scripts).toEqual([
      "Set-Item -LiteralPath 'Env:NPM_CONFIG_REGISTRY' -Value 'http://192.0.2.2:48123/first'\nWrite-Output $env:NPM_CONFIG_REGISTRY",
      "Set-Item -LiteralPath 'Env:NPM_CONFIG_REGISTRY' -Value 'http://192.0.2.2:48123/second''s'\nWrite-Output $env:NPM_CONFIG_REGISTRY",
    ]);
  });

  it("carries Windows background environment before the detached command", async () => {
    let uploadedScript = "";
    const runCommand = (_command: string, _args: string[], options?: { input?: string }) => {
      uploadedScript = options?.input ?? "";
      return { status: 1, stderr: "fixture upload failure", stdout: "" };
    };
    await expect(
      runWindowsBackgroundPowerShell({
        env: { NPM_CONFIG_REGISTRY: "http://192.0.2.2:48123/candidate's" },
        label: "environment proof",
        runCommand,
        script: "Write-Output $env:NPM_CONFIG_REGISTRY",
        timeoutMs: 720_000,
        vmName: "Windows VM",
      }),
    ).rejects.toThrow("background script write failed");
    expect(uploadedScript).toContain(
      "Set-Item -LiteralPath 'Env:NPM_CONFIG_REGISTRY' -Value 'http://192.0.2.2:48123/candidate''s'\nWrite-Output $env:NPM_CONFIG_REGISTRY",
    );
  });

  it("paces ambiguous Windows background launch materialization probes", async () => {
    let calls = 0;
    const runCommand = vi.fn(() => {
      calls++;
      return { status: 0, stderr: "", stdout: "" };
    });

    await expect(
      runWindowsBackgroundPowerShell({
        label: "ambiguous launch",
        pollIntervalMs: 20,
        runCommand,
        script: "Write-Output ok",
        timeoutMs: 90,
        vmName: "Windows 11",
      }),
    ).rejects.toThrow("ambiguous launch background launch failed");

    expect(calls).toBeLessThan(20);
  });

  it("fails fast when a Windows Parallels VM stops during background work", async () => {
    const runCommand = vi.fn(() => ({
      status: 1,
      stderr:
        'Unable to perform the operation because "Windows 11" is not started. This operation can be performed for running virtual machines only.',
      stdout: "",
    }));

    await expect(
      runWindowsBackgroundPowerShell({
        label: "ref-onboard",
        runCommand,
        script: "Write-Output ok",
        timeoutMs: 720_000,
        vmName: "Windows 11",
      }),
    ).rejects.toThrow("ref-onboard failed: Parallels VM stopped");

    expect(runCommand).toHaveBeenCalledTimes(1);
  });

  it("fails boundedly when Windows background polling loses guest transport", async () => {
    const retries: string[] = [];
    let donePolls = 0;
    const runCommand = vi.fn((_command: string, args: string[], options?: { input?: string }) => {
      const command = args.at(-1) ?? "";
      if (options?.input) {
        return { status: 0, stderr: "", stdout: "" };
      }
      if (args.includes("--current-user")) {
        return { status: 0, stderr: "", stdout: "started\n" };
      }
      if (args.includes("cmd.exe") && command.includes("echo wait")) {
        donePolls++;
        return { status: 124, stderr: "", stdout: "" };
      }
      return { status: 0, stderr: "", stdout: "" };
    });

    await expect(
      runWindowsBackgroundPowerShell({
        label: "ref-onboard",
        onLaunchRetry: (message) => retries.push(message),
        pollIntervalMs: 1,
        runCommand,
        script: "Write-Output ok",
        timeoutMs: 720_000,
        vmName: "Windows 11",
      }),
    ).rejects.toThrow("ref-onboard done poll failed after 3 consecutive guest transport errors");

    expect(donePolls).toBe(3);
    expect(retries).toHaveLength(3);
    expect(retries.at(-1)).toContain("transport failure 3/3");
  });
});
