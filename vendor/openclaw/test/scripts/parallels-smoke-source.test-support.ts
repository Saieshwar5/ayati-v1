import { readFileSync } from "node:fs";

export const WRAPPERS = {
  linux: "scripts/e2e/parallels-linux-smoke.sh",
  npmUpdate: "scripts/e2e/parallels-npm-update-smoke.sh",
};

const PROVIDER_AUTH_PREREQUISITE_PATH = "scripts/e2e/parallels/provider-auth-prerequisite.mjs";

export const PROVIDER_AUTH_PREREQUISITE_SOURCE = readFileSync(
  PROVIDER_AUTH_PREREQUISITE_PATH,
  "utf8",
);

export const TS_PATHS = {
  agentWorkspace: "scripts/e2e/parallels/agent-workspace.ts",
  common: "scripts/e2e/parallels/common.ts",
  guestTransports: "scripts/e2e/parallels/guest-transports.ts",
  hostCommand: "scripts/e2e/parallels/host-command.ts",
  hostServer: "scripts/e2e/parallels/host-server.ts",
  linux: "scripts/e2e/parallels/linux-smoke.ts",
  macosDiscord: "scripts/e2e/parallels/macos-discord.ts",
  macos: "scripts/e2e/parallels/macos-smoke.ts",
  npmUpdateScripts: "scripts/e2e/parallels/npm-update-scripts.ts",
  npmUpdate: "scripts/e2e/parallels/npm-update-smoke.ts",
  packageArtifact: "scripts/e2e/parallels/package-artifact.ts",
  parallelsVm: "scripts/e2e/parallels/parallels-vm.ts",
  phaseRunner: "scripts/e2e/parallels/phase-runner.ts",
  powershell: "scripts/e2e/parallels/powershell.ts",
  providerAuth: "scripts/e2e/parallels/provider-auth.ts",
  snapshots: "scripts/e2e/parallels/snapshots.ts",
  smokeCommon: "scripts/e2e/parallels/smoke-common.ts",
  windows: "scripts/e2e/parallels/windows-smoke.ts",
  windowsGit: "scripts/e2e/parallels/windows-git.ts",
};

export const TS_SOURCE = Object.fromEntries(
  Object.entries(TS_PATHS).map(([name, filePath]) => [name, readFileSync(filePath, "utf8")]),
) as Record<keyof typeof TS_PATHS, string>;

export const OS_TS_PATHS = [TS_PATHS.linux, TS_PATHS.macos, TS_PATHS.windows];
