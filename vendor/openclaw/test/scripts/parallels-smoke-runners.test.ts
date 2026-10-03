import { spawnSync } from "node:child_process";
import { chmodSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { useAutoCleanupTempDirTracker } from "../helpers/temp-dir.js";
import {
  fakePrlctlEnv,
  writeNodeFakePrlctl,
  testNodeExecPath,
} from "./parallels-smoke-command.test-support.js";
import { TS_PATHS, TS_SOURCE, OS_TS_PATHS } from "./parallels-smoke-source.test-support.js";

const {
  agentWorkspace: workspace,
  guestTransports: transports,
  linux,
  macos,
  macosDiscord: discord,
  npmUpdateScripts,
  powershell,
  smokeCommon,
  windows,
  windowsGit,
} = TS_SOURCE;

const tempDirs = useAutoCleanupTempDirTracker(afterEach);

describe("Parallels smoke runners", () => {
  it("resets Linux product state before both install lanes", () => {
    for (const lane of ["fresh", "upgrade"]) {
      const restoreIndex = linux.indexOf(`"${lane}.restore-snapshot"`);
      const resetIndex = linux.indexOf(`"${lane}.reset-state"`);
      const installIndex = linux.indexOf(
        `"${lane}.${lane === "fresh" ? "install-main" : "install-latest"}"`,
      );
      expect(restoreIndex).toBeGreaterThanOrEqual(0);
      expect(resetIndex).toBeGreaterThan(restoreIndex);
      expect(installIndex).toBeGreaterThan(resetIndex);
    }
    expect(linux).toContain("npm uninstall -g openclaw");
    expect(linux).toContain("rm -rf /root/.openclaw /root/.npm/_cacache");
  });

  it("uses a forced Windows gateway stop only when the installed CLI supports it", () => {
    expect(windows).toContain("Invoke-OpenClaw gateway stop --help");
    expect(windows).toContain("$stopHelp -match");
    expect(windows).toContain("$gatewayArgs += '--force'");
    expect(windows).toContain("Invoke-OpenClaw @gatewayArgs");
    expect(windows).not.toContain('const forceFlag = action === "stop"');
  });

  it("starts the default macOS upgrade without host networking or packaging the checkout", () => {
    const tempDir = tempDirs.make("openclaw-macos-upgrade-no-pack-");
    writeNodeFakePrlctl(
      tempDir,
      `if (args.includes("list")) {
         console.log(JSON.stringify([{ name: "macOS Tahoe", status: "running" }]));
         process.exit(0);
       }
       if (args.includes("exec") && args.includes("whoami")) {
         console.log("runner");
         process.exit(0);
       }
       process.stderr.write("FAKE_GUEST_COMMAND_BOUNDARY\\n");
       process.exit(73);`,
    );
    const fakePnpm = join(tempDir, "pnpm");
    writeFileSync(
      fakePnpm,
      '#!/usr/bin/env node\nprocess.stderr.write("UNEXPECTED_HOST_PACKAGE_BUILD\\n"); process.exit(86);\n',
    );
    chmodSync(fakePnpm, 0o755);

    const result = spawnSync(
      testNodeExecPath,
      [
        "--import",
        "tsx",
        TS_PATHS.macos,
        "--mode",
        "upgrade",
        "--latest-version",
        "2026.8.25",
        "--api-key-env",
        "OPENCLAW_PARALLELS_TEST_API_KEY",
        "--json",
      ],
      {
        cwd: process.cwd(),
        encoding: "utf8",
        env: {
          ...process.env,
          ...fakePrlctlEnv(tempDir),
          "BASH_FUNC_ifconfig%%": "() { return 1; }",
          OPENCLAW_PARALLELS_ARTIFACT_ROOT: join(tempDir, "artifacts"),
          OPENCLAW_PARALLELS_SKIP_SNAPSHOT_RESTORE: "1",
          OPENCLAW_PARALLELS_TEST_API_KEY: "fixture-not-a-real-credential",
          TMPDIR: tempDir,
          npm_execpath: fakePnpm,
        },
      },
    );

    expect(result.stderr).toContain("upgrade.restore-snapshot");
    expect(result.stderr).toContain("upgrade.reset-state");
    expect(result.stderr).toContain("FAKE_GUEST_COMMAND_BOUNDARY");
    expect(result.stderr).not.toContain("UNEXPECTED_HOST_PACKAGE_BUILD");
  });

  it("waits for apt locks during Linux snapshot bootstrap", () => {
    expect(linux).toContain("APT_LOCK_RETRY_SECONDS = 900");
    expect(linux).toContain("BOOTSTRAP_TIMEOUT_SECONDS = 1200");
    expect(linux).toContain("command -v wget");
    expect(linux).toContain("run_apt_with_lock_retry");
    expect(linux).toContain('"Could not get lock"');
    expect(linux).toContain('"Unable to acquire the dpkg frontend lock"');
    expect(linux).toContain('"Unable to lock directory"');
    expect(linux).toContain("downloadGuestFile");
    expect(linux).toContain("this.downloadGuestFile(tgzUrl");
    expect(linux).toContain("curl -fsSL --connect-timeout 10 --max-time 120 --retry 2");
    expect(linux).toContain("wget -q --timeout=10 --read-timeout=120 --tries=3");
  });

  it("keeps Linux bad-plugin diagnostics gated for historical update baselines", () => {
    expect(linux).toContain('BAD_PLUGIN_DIAGNOSTIC_MIN_VERSION = "2026.5.7"');
    expect(linux).toContain("parseOpenClawPackageVersion");
    expect(linux).toContain("maybeInjectBadPluginFixture");
    expect(linux).toContain("maybeVerifyBadPluginDiagnostic");
    expect(linux).toContain("Skipping bad plugin diagnostic fixture");
    expect(linux).toContain("Skipping bad plugin diagnostic assertion");
  });

  it("seeds agent workspace state before OS smoke agent turns", () => {
    // workspace-state.json was retired (b6535fb8de5: stop writing retired
    // smoke state); identity/bootstrap seeding remains the contract.
    expect(workspace).not.toContain("workspace-state.json");
    expect(workspace).toContain("IDENTITY.md");
    expect(workspace).toContain("BOOTSTRAP.md");

    for (const scriptPath of OS_TS_PATHS) {
      const script = readFileSync(scriptPath, "utf8");

      expect(script, scriptPath).toContain("AgentWorkspaceScript");
      expect(script, scriptPath).toContain("parallels-");
      if (scriptPath !== TS_PATHS.windows) {
        expect(script, scriptPath).toContain("agents.defaults.skipBootstrap");
        expect(script, scriptPath).toContain("tools.profile");
      }
      expect(script, scriptPath).toContain("--thinking");
      expect(script, scriptPath).toContain("off");
      expect(script, scriptPath).toContain(
        scriptPath === TS_PATHS.windows
          ? "finalAssistant(Raw|Visible)Text"
          : "posixAgentTurnScript({",
      );
    }
    expect(smokeCommon).toContain("finalAssistant(Raw|Visible)Text");
    expect(macos).toContain("modelProviderConfigBatchJson");
    expect(macos).toContain("config set --batch-file");
    expect(linux).toContain("modelProviderConfigBatchJson");
    expect(linux).toContain("config set --batch-file");
    expect(windows).toContain("windowsAgentTurnConfigPatchScript");
    expect(powershell).toContain("agents.defaults.skipBootstrap");
    expect(powershell).toContain("tools.profile");
    expect(powershell).toContain("replace(/^\\\\uFEFF/u");

    expect(npmUpdateScripts).toContain("posixAgentWorkspaceScript");
    expect(npmUpdateScripts).toContain("windowsAgentWorkspaceScript");
    expect(npmUpdateScripts).toContain("tools.profile");
    expect(npmUpdateScripts).toContain("--thinking off");
    expect(npmUpdateScripts).toContain("finalAssistant(Raw|Visible)Text");
    expect(npmUpdateScripts).toContain("posixAssertAgentOkScript");
    expect(npmUpdateScripts).toContain("posixAgentTurnScript({");
    expect(npmUpdateScripts).toContain("windowsAgentTurnConfigPatchScript");
    expect(npmUpdateScripts).toContain("modelProviderConfigBatchJson");
    expect(npmUpdateScripts).toContain("config set --batch-file");
  });

  it("hardens restored macOS install lanes", () => {
    expect(macos).toContain('rm -rf "$HOME/.npm/_cacache"');
    expect(macos.match(/\.onboard-ref", 420/g)).toHaveLength(2);
    expect(macos).toContain('echo "npm install attempt $attempt failed; retrying in 5s"');
    expect(macos.match(/curl -fsSL --connect-timeout 10 --max-time 120 --retry 2/g)).toHaveLength(
      2,
    );
  });

  it("provisions portable Git before Windows dev update lanes", () => {
    const combined = `${windows}\n${windowsGit}`;

    expect(windows).toContain("prepareMinGitZip");
    expect(windows).toContain("ensureGuestGit");
    expect(windows).toContain("fresh.ensure-git");
    expect(windows).toContain("upgrade.ensure-git");
    expect(combined).toContain("MinGit-");
    expect(combined).toContain("portable-git");
    expect(combined).toContain("where.exe git.exe");
    expect(windowsGit.indexOf('"MinGit-2.55.0.5-64-bit.zip"')).toBeLessThan(
      windowsGit.indexOf('"MinGit-2.55.0.5-arm64.zip"'),
    );
    expect(
      combined.match(/curl\.exe -fsSL --connect-timeout 10 --max-time 120 --retry 2/g),
    ).toHaveLength(2);
    expect(windows).toContain("Invoke-RestMethod -Uri");
    expect(windows).toContain("-TimeoutSec 120");
    expect(windowsGit).toContain('if "-64-bit." in name:');
    expect(windowsGit).toContain('elif "-arm64." in name:');
  });

  it("preseeds dev update channel before stable-to-dev update lanes", () => {
    expect(macos).toContain('channel: "dev"');
    expect(windows).toContain("Name channel -Value 'dev'");
    expect(macos).toContain("OPENCLAW_ALLOW_OLDER_BINARY_DESTRUCTIVE_ACTIONS=1");
    expect(windows).toContain("OPENCLAW_ALLOW_OLDER_BINARY_DESTRUCTIVE_ACTIONS");
  });

  it("requires macOS dashboard smoke to load built assets", () => {
    expect(macos).toContain("asset_paths=");
    expect(macos).toContain("grep -E '(^|/)assets/'");
    expect(macos).toContain('curl -fsSL --connect-timeout 2 --max-time 5 "$asset_url"');
  });

  it("keeps macOS Discord roundtrip isolated from the lane orchestrator", () => {
    expect(macos).toContain("MacosDiscordSmoke");
    expect(macos).not.toContain("Authorization: Bot");
    expect(discord).toContain("Authorization: Bot");
    expect(discord).toContain('import { randomUUID } from "node:crypto"');
    expect(discord).not.toContain("Math.random()");
    expect(discord).toContain('"--silent"');
    expect(discord).toContain("doctor --fix --yes --non-interactive");
    expect(discord).toContain("channels status --probe --json");
    expect(discord).toContain("Stop ${this.input.vmName} after successful Discord smoke");
  });

  it("resolves macOS smoke commands from the guest PATH", () => {
    expect(macos).toContain("/usr/local/bin:/usr/local/sbin");
    expect(macos).toContain('const guestOpenClaw = "openclaw"');
    expect(macos).toContain('const guestNode = "node"');
    expect(macos).toContain('const guestNpm = "npm"');
    expect(macos).toContain("$(npm root -g)/openclaw/openclaw.mjs");
    expect(macos).toContain("guestOpenClawEntryExec");
    expect(macos).not.toContain('const guestOpenClaw = "/opt/homebrew/bin/openclaw"');
    expect(macos).not.toContain('const guestNode = "/opt/homebrew/bin/node"');
    expect(macos).not.toContain('const guestNpm = "/opt/homebrew/bin/npm"');
    expect(macos).not.toContain("/opt/homebrew/lib/node_modules/openclaw/openclaw.mjs");
  });

  it("keeps Windows gateway reachability on a real deadline with start recovery", () => {
    expect(windows).toContain("OPENCLAW_PARALLELS_WINDOWS_GATEWAY_RECOVERY_AFTER_S");
    expect(windows).toContain("Date.now() < deadline");
    expect(windows).toContain("gateway start");
    expect(windows).toContain("gateway-reachable recovery");
  });

  it("runs Windows package installs through the detached done-file runner", () => {
    expect(windows).toContain('guestPowerShellBackground(\n      "install-latest"');
    expect(windows).toContain("guestPowerShellBackground(\n      `install-main-${");
    expect(windows).toContain('guestPowerShellBackground(\n      "update-dev"');
    expect(windows).not.toMatch(/private installMain\(tempName: string\): void/u);
    expect(windows).not.toMatch(/private installLatestRelease\(\): void/u);
    expect(windows).not.toMatch(/private runDevChannelUpdate\(\): void/u);
    expect(windows).toContain("if (Test-Path $configPath)");
    expect(windows).toContain(
      "New-Item -ItemType Directory -Path (Split-Path $configPath -Parent) -Force",
    );
  });

  it("runs the Windows agent turn through the detached done-file runner", () => {
    expect(windows).toContain('guestPowerShellBackground(\n      "agent-turn"');
    expect(windows).toContain("OPENCLAW_PARALLELS_WINDOWS_AGENT_TIMEOUT_S");
    expect(windows).toContain(
      'readPositiveIntEnv(\n    "OPENCLAW_PARALLELS_WINDOWS_AGENT_TIMEOUT_S"',
    );
    expect(windows).toContain("windowsAgentTurnConfigPatchScript(this.auth.modelId)");
    expect(windows).toContain("--model");
    expect(windows).toContain('resolveParallelsModelTimeoutSeconds("windows")');
    expect(windows).toContain("finalAssistant(Raw|Visible)Text");
    expect(windows).toContain("parallels-windows-smoke-retry-$attempt");
    expect(windows).toContain("agent turn attempt $attempt failed or finished without OK response");
    expect(windows).not.toContain("$config.models.providers");
    expect(windows).not.toContain("timeoutSeconds = 300");
    expect(windows).toContain('"$sessionId.jsonl"');
  });

  it("waits through transient Windows restoring state before VM operations", () => {
    expect(windows).toContain("waitForVmNotRestoring");
    expect(windows).toContain("snapshot-switch retry");
    expect(transports).toContain("launch retry");
  });

  it("preserves bundled plugin inventory during dev updates", () => {
    const devUpdateLines = [macos, windows].map((script) =>
      script.split("\n").find((line) => line.includes("update --channel dev")),
    );

    expect(devUpdateLines).not.toContain(undefined);
    for (const updateLine of devUpdateLines) {
      expect(updateLine).not.toContain("OPENCLAW_DISABLE_BUNDLED_PLUGINS");
    }
    expect(powershell).toContain("windowsScopedEnvFunction");
    expect(windows).toContain(
      "Invoke-WithScopedEnv @{ OPENCLAW_ALLOW_OLDER_BINARY_DESTRUCTIVE_ACTIONS",
    );
    expect(windows).toContain("$script:OpenClawUpdateExit = $LASTEXITCODE");
    expect(windows).not.toContain("$env:OPENCLAW_DISABLE_BUNDLED_PLUGINS = '1'");
    for (const script of [macos, windows]) {
      expect(script).toContain('readGitCommitEnv("OPENCLAW_PARALLELS_DEV_TARGET_REF")');
      expect(script).toContain("OPENCLAW_UPDATE_DEV_TARGET_REF");
      expect(script).toContain("assertDevChannelUpdate(status, this.devTargetCommit, () =>");
    }
    expect(smokeCommon).toContain('const expectedBranch = targetCommit ? "HEAD" : "main"');
    expect(smokeCommon).toContain("dev update checkout head");
    expect(macos).toContain("OPENCLAW_UPDATE_DEV_TARGET_REF=${shellQuote(this.devTargetCommit)}");
    expect(windows).toContain(
      "OPENCLAW_UPDATE_DEV_TARGET_REF = ${psSingleQuote(this.devTargetCommit)}",
    );
  });

  it("keeps Parallels dev updates on the test-owned gateway lifecycle", () => {
    expect(macos).toContain(
      "update --channel dev --yes --json --no-restart --timeout ${this.updateDevTimeoutSeconds}",
    );
    expect(windows).toContain(
      "update --channel dev --yes --json --no-restart --timeout ${this.updateTimeoutSeconds}",
    );
    expect(macos).toContain("--install-daemon");
    expect(windows).toContain("--install-daemon");
  });

  it("resolves Windows OpenClaw commands without assuming the npm shim path", () => {
    expect(powershell).toContain("windowsOpenClawResolver");
    expect(powershell).toContain("OPENCLAW_PARALLELS_AGENT_RUNTIME_POLICY_SUPPORTED");
    expect(powershell).toContain("Programs\\nodejs");
    expect(powershell).toContain('selectedModelEntry.agentRuntime = { id: "openclaw" }');
    expect(powershell).toContain("delete selectedModelEntry.agentRuntime");
    expect(powershell).toContain("delete providerEntry.agentRuntime");
    expect(powershell).toContain("Resolve-OpenClawCommand");
    expect(powershell).toContain("npm\\node_modules\\openclaw\\openclaw.mjs");
    expect(powershell).toContain("$ErrorActionPreference = 'Continue'");
    expect(powershell).toContain("$PSNativeCommandUseErrorActionPreference = $false");
    expect(windows).toContain("windowsOpenClawResolver");
    expect(windows).toContain("Invoke-OpenClaw gateway");
    expect(windows).not.toContain("Join-Path $env:APPDATA 'npm\\\\openclaw.cmd'");
  });
});
