import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { delimiter, join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  isLikelyMacosDesktopHome,
  parseMacosDsclUserHomeLine,
  resolveHostIp,
  resolveMacosVmName,
  resolveSnapshot,
  ensureVmRunning,
  shouldSkipSnapshotRestore,
  resolveUbuntuVmName,
  shellQuote,
  SKIP_SNAPSHOT_RESTORE_ENV,
  validateSnapshotRestoreMode,
} from "../../scripts/e2e/parallels/common.ts";
import { scriptProcessEntrypoints } from "../../scripts/script-process-runtime.test-support.js";
import {
  resolveRuntimeWorkerArgv,
  resolveRuntimeWorkerUrl,
} from "../../src/infra/runtime-worker-url.js";
import { withEnv } from "../../src/test-utils/env.js";
import { spawnNodeEvalSync } from "../../src/test-utils/node-process.js";
import { useAutoCleanupTempDirTracker } from "../helpers/temp-dir.js";
import {
  fakePrlctlEnv,
  writeNodeFakePrlctl,
  testNodeExecPath,
} from "./parallels-smoke-command.test-support.js";
import { TS_PATHS } from "./parallels-smoke-source.test-support.js";

const tempDirs = useAutoCleanupTempDirTracker(afterEach);

function writeFakeHostIpCommand(tempDir: string, name: string, body: string): void {
  const commandPath = join(tempDir, name);
  writeFileSync(commandPath, `#!/bin/sh\n${body}\n`);
  chmodSync(commandPath, 0o755);
}

function withFakeHostIpCommands<T>(
  input: {
    ifconfigOutput?: string;
    ifconfigStatus?: number;
    prlsrvctlOutput?: string;
    prlsrvctlStatus?: number;
  },
  runTest: (callsPath: string) => T,
): T {
  const tempDir = tempDirs.make("openclaw-parallels-host-ip-");
  const callsPath = join(tempDir, "calls.log");
  writeFakeHostIpCommand(
    tempDir,
    "prlsrvctl",
    'printf "prlsrvctl:%s:%s\\n" "$LC_ALL" "$*" >>"$OPENCLAW_HOST_IP_CALLS"\nprintf "%b" "$OPENCLAW_PRLSRVCTL_OUTPUT"\nexit "$OPENCLAW_PRLSRVCTL_STATUS"',
  );
  writeFakeHostIpCommand(
    tempDir,
    "ifconfig",
    'printf "ifconfig:%s\\n" "$*" >>"$OPENCLAW_HOST_IP_CALLS"\nprintf "%b" "$OPENCLAW_IFCONFIG_OUTPUT"\nexit "$OPENCLAW_IFCONFIG_STATUS"',
  );
  return withEnv(
    {
      LC_ALL: "fixture-locale",
      OPENCLAW_HOST_IP_CALLS: callsPath,
      OPENCLAW_IFCONFIG_OUTPUT: input.ifconfigOutput ?? "",
      OPENCLAW_IFCONFIG_STATUS: String(input.ifconfigStatus ?? 0),
      OPENCLAW_PRLSRVCTL_OUTPUT: input.prlsrvctlOutput ?? "",
      OPENCLAW_PRLSRVCTL_STATUS: String(input.prlsrvctlStatus ?? 0),
      PATH: `${tempDir}${delimiter}${process.env.PATH ?? ""}`,
    },
    () => runTest(callsPath),
  );
}

function writeJsonFakePrlctl(tempDir: string, routes: Record<string, unknown>): void {
  const routeJson = JSON.stringify(routes);
  writeNodeFakePrlctl(
    tempDir,
    `const routes = ${routeJson}; for (const [command, payload] of Object.entries(routes)) { if (args.some((arg) => arg.includes(command))) { console.log(JSON.stringify(payload)); process.exit(0); } } process.exit(1);`,
  );
}

function withJsonFakePrlctl<T>(routes: Record<string, unknown>, runTest: () => T): T {
  const tempDir = tempDirs.make("openclaw-parallels-prlctl-");
  writeJsonFakePrlctl(tempDir, routes);
  return withEnv(fakePrlctlEnv(tempDir), runTest);
}

describe("Parallels smoke vm", () => {
  it("parses macOS dscl user homes with spaces on mounted volumes", () => {
    expect(parseMacosDsclUserHomeLine("clawuser /Volumes/Macintosh HD/Users/clawuser")).toEqual({
      user: "clawuser",
      home: "/Volumes/Macintosh HD/Users/clawuser",
    });
    expect(isLikelyMacosDesktopHome("/Volumes/Macintosh HD/Users/clawuser")).toBe(true);
    expect(isLikelyMacosDesktopHome("/var/empty")).toBe(false);
  });

  describe.skipIf(process.platform === "win32")("Parallels host IP detection", () => {
    it("keeps an explicit host IP above both detectors", () => {
      withFakeHostIpCommands({}, (callsPath) => {
        expect(resolveHostIp("192.0.2.10")).toBe("192.0.2.10");
        expect(existsSync(callsPath)).toBe(false);
      });
    });

    it("reads the configured Shared adapter before any live interface exists", () => {
      const output = `Network ID: Shared
Type: shared
Parallels adapter:
\tIPv4 address: 10.211.55.2
\tIPv4 subnet mask: 255.255.255.0
DHCPv4 server:
\tServer address: 10.211.55.1
`;
      withFakeHostIpCommands({ ifconfigStatus: 1, prlsrvctlOutput: output }, (callsPath) => {
        expect(resolveHostIp()).toBe("10.211.55.2");
        expect(readFileSync(callsPath, "utf8")).toBe("prlsrvctl:C:net info Shared\n");
      });
    });

    it("accepts CRLF and harmless whitespace in Shared network output", () => {
      const output =
        "Network ID: Shared\r\n  Parallels adapter:  \r\n    IPv4 address:   10.211.55.3  \r\nDHCPv4 server:\r\n";
      withFakeHostIpCommands({ prlsrvctlOutput: output }, () => {
        expect(resolveHostIp()).toBe("10.211.55.3");
      });
    });

    it.each(["", "    IPv4 address: not-an-ip\n"])(
      "falls back to ifconfig when the adapter address is missing or malformed",
      (adapterLine) => {
        const output = `Network ID: Shared
Parallels adapter:
${adapterLine}DHCPv4 server:
    Server address: 10.211.55.1
`;
        withFakeHostIpCommands(
          {
            ifconfigOutput: "vnic0: flags=8843<UP>\n\tinet 10.211.55.9 netmask 0xffffff00\n",
            prlsrvctlOutput: output,
          },
          (callsPath) => {
            expect(resolveHostIp()).toBe("10.211.55.9");
            expect(readFileSync(callsPath, "utf8")).toBe(
              "prlsrvctl:C:net info Shared\nifconfig:\n",
            );
          },
        );
      },
    );

    it("preserves ifconfig fallback when Shared network lookup fails", () => {
      withFakeHostIpCommands(
        {
          ifconfigOutput: "bridge100: flags=8863<UP>\n\tinet 10.211.55.7 netmask 0xffffff00\n",
          prlsrvctlStatus: 1,
        },
        (callsPath) => {
          expect(resolveHostIp()).toBe("10.211.55.7");
          expect(readFileSync(callsPath, "utf8")).toBe("prlsrvctl:C:net info Shared\nifconfig:\n");
        },
      );
    });
  });

  it("quotes shell args and resolves fuzzy snapshot hints through the shared TypeScript helper", () => {
    const output = withJsonFakePrlctl(
      {
        "snapshot-list": {
          "{older}": { name: "fresh", state: "running" },
          "{wanted}": { name: "fresh-poweroff-2026-04-01", state: "poweroff" },
          "{old-e2e}": {
            name: "pre-openclaw-native-e2e-2026-03-12",
            state: "poweroff",
            date: "2026-03-12 22:32:24",
          },
          "{new-e2e}": {
            name: "pre-openclaw-native-e2e-2026-07-26",
            state: "poweroff",
            date: "2026-07-26 11:52:02",
          },
          "{undated-first}": { name: "undated-family-1", state: "poweroff" },
          "{dated-later}": {
            name: "undated-family-2",
            state: "poweroff",
            date: "2026-07-26 11:52:02",
          },
          "{other}": { name: "unrelated", state: "poweroff" },
        },
      },
      () => {
        const snapshot = resolveSnapshot("vm", "fresh");
        const latestE2e = resolveSnapshot("vm", "pre-openclaw-native-e2e-");
        const missingDate = resolveSnapshot("vm", "undated-family-");
        return [
          shellQuote("it's ok"),
          [snapshot.id, snapshot.state, snapshot.name].join("\t"),
          [latestE2e.id, latestE2e.state, latestE2e.name].join("\t"),
          [missingDate.id, missingDate.state, missingDate.name].join("\t"),
        ].join("\n");
      },
    );

    expect(output.split("\n")[0]).toBe("'it'\"'\"'s ok'");
    expect(output).toContain("{wanted}\tpoweroff\tfresh-poweroff-2026-04-01");
    expect(output).toContain("{new-e2e}\tpoweroff\tpre-openclaw-native-e2e-2026-07-26");
    expect(output).toContain("{undated-first}\tpoweroff\tundated-family-1");
  });

  it("resolves a latest snapshot hint to the matching version before older LATEST labels", () => {
    const output = withJsonFakePrlctl(
      {
        "snapshot-list": {
          "{old}": { name: "macOS 26.3.1 LATEST", state: "poweron" },
          "{wanted}": { name: "macOS 26.5", state: "poweron" },
        },
      },
      () => {
        const snapshot = resolveSnapshot("vm", "macOS 26.5 latest");
        return [snapshot.id, snapshot.state, snapshot.name].join("\t");
      },
    );

    expect(output).toBe("{wanted}\tpoweron\tmacOS 26.5");
  });

  it.each(["poweron", "poweroff"] as const)(
    "restores a %s macOS snapshot without discarding its saved desktop session",
    (snapshotState) => {
      const tempDir = tempDirs.make("openclaw-parallels-macos-restore-");
      const callsPath = join(tempDir, "prlctl-calls.jsonl");
      const statePath = join(tempDir, "vm-state");
      writeNodeFakePrlctl(
        tempDir,
        `const fs = process.getBuiltinModule("node:fs");
const callsPath = ${JSON.stringify(callsPath)};
const statePath = ${JSON.stringify(statePath)};
const snapshotState = ${JSON.stringify(snapshotState)};
const commandIndex = args.findIndex((arg) =>
  ["list", "snapshot-list", "snapshot-switch", "status", "start", "exec"].includes(arg),
);
const commandArgs = args.slice(commandIndex);
fs.appendFileSync(callsPath, JSON.stringify(commandArgs) + "\\n");
if (commandArgs[0] === "list") {
  console.log(JSON.stringify([{ name: "macOS Tahoe", status: "running" }]));
} else if (commandArgs[0] === "snapshot-list") {
  console.log(JSON.stringify({ "{snapshot}": { name: "macOS 26.5 latest", state: snapshotState } }));
} else if (commandArgs[0] === "snapshot-switch") {
  fs.writeFileSync(statePath, snapshotState === "poweroff" || commandArgs.includes("--skip-resume") ? "stopped" : "running");
} else if (commandArgs[0] === "status") {
  console.log("VM macOS Tahoe " + fs.readFileSync(statePath, "utf8"));
} else if (commandArgs[0] === "start") {
  fs.writeFileSync(statePath, "running");
} else if (commandArgs[0] === "exec" && commandArgs.includes("whoami")) {
  console.log("desktop-user");
} else if (commandArgs[0] === "exec" && commandArgs.includes("/bin/dd")) {
  process.exit(41);
}`,
      );

      const result = spawnSync(
        testNodeExecPath,
        [
          // Darwin exercises the source-relative Python transport beside the script.
          ...(process.platform === "darwin"
            ? ["--import", "tsx", TS_PATHS.macos]
            : resolveRuntimeWorkerArgv(
                resolveRuntimeWorkerUrl(scriptProcessEntrypoints.macosSmoke),
                testNodeExecPath,
              )),
          "--mode",
          "upgrade",
          "--latest-version",
          "2026.1.1",
          "--api-key-env",
          "OPENCLAW_PARALLELS_TEST_KEY",
          "--json",
        ],
        {
          cwd: process.cwd(),
          encoding: "utf8",
          env: {
            ...process.env,
            ...fakePrlctlEnv(tempDir),
            OPENCLAW_PARALLELS_ARTIFACT_ROOT: tempDir,
            OPENCLAW_PARALLELS_TEST_KEY: "fixture",
          },
          timeout: 20_000,
        },
      );

      expect(result.status, result.stderr).toBe(1);
      expect(result.stderr).toContain("macOS guest command failed with exit code 41");
      const calls = readFileSync(callsPath, "utf8")
        .trim()
        .split("\n")
        .map((line) => JSON.parse(line) as string[]);
      expect(
        calls.find(([command]) => command === "snapshot-switch"),
        result.stderr,
      ).toEqual(["snapshot-switch", "macOS Tahoe", "--id", "{snapshot}"]);
      expect(calls.filter(([command]) => command === "start")).toHaveLength(
        snapshotState === "poweroff" ? 1 : 0,
      );
    },
    30_000,
  );

  it("rejects skip-restore for combined Parallels smoke lanes", () => {
    expect(withEnv({ [SKIP_SNAPSHOT_RESTORE_ENV]: "1" }, () => shouldSkipSnapshotRestore())).toBe(
      true,
    );
    const invalidSkipBothResult = spawnNodeEvalSync(
      `process.env.${SKIP_SNAPSHOT_RESTORE_ENV} = "1"; const { validateSnapshotRestoreMode } = await import("./${TS_PATHS.common}"); validateSnapshotRestoreMode("both", "test smoke");`,
      { env: process.env, imports: ["tsx"] },
    );
    expect(invalidSkipBothResult.status).toBe(1);
    expect(invalidSkipBothResult.stderr).toContain(
      "OPENCLAW_PARALLELS_SKIP_SNAPSHOT_RESTORE=1 requires --mode fresh or --mode upgrade",
    );
    expect(() =>
      withEnv({ [SKIP_SNAPSHOT_RESTORE_ENV]: "1" }, () =>
        validateSnapshotRestoreMode("fresh", "test smoke"),
      ),
    ).not.toThrow();
    expect(() => validateSnapshotRestoreMode("both", "test smoke")).not.toThrow();
  });

  it("uses one Ubuntu VM fallback resolver for Linux lanes", () => {
    const output = withJsonFakePrlctl(
      {
        list: [
          { name: "Ubuntu 9007199254740993.04" },
          { name: "Ubuntu 26.04" },
          { name: "Ubuntu 25.10" },
          { name: "Ubuntu 23.10" },
          { name: "Ubuntu 24.04.3 ARM64" },
        ],
      },
      () => resolveUbuntuVmName("Ubuntu missing"),
    );

    expect(output).toBe("Ubuntu 26.04");
  });

  it("skips unsafe Ubuntu version names in fallback resolver", () => {
    const output = withJsonFakePrlctl(
      {
        list: [{ name: "Ubuntu 9007199254740993.04" }, { name: "Ubuntu 23.10" }],
      },
      () => resolveUbuntuVmName("Ubuntu missing"),
    );

    expect(output).toBe("Ubuntu 23.10");
  });

  it("uses the only macOS VM when the default name is unavailable", () => {
    const output = withJsonFakePrlctl({ list: [{ name: "Windows 11" }, { name: "macOS" }] }, () =>
      resolveMacosVmName("macOS Tahoe"),
    );

    expect(output).toBe("macOS");
  });

  it("does not infer destructive macOS smoke targets from arbitrary names", () => {
    const result = withJsonFakePrlctl({ list: [{ name: "macOS Work" }] }, () =>
      spawnNodeEvalSync(
        `const { resolveMacosVmName } = await import("./${TS_PATHS.parallelsVm}"); resolveMacosVmName("macOS Tahoe");`,
        { env: process.env, imports: ["tsx"] },
      ),
    );

    expect(result.status).toBe(1);
    expect(result.stderr).toContain("select a macOS VM explicitly");
  });

  it("resumes suspended Parallels VMs", () => {
    const tempDir = tempDirs.make("openclaw-parallels-vm-resume-");
    const statePath = join(tempDir, "state");
    writeFileSync(statePath, "suspended");
    writeNodeFakePrlctl(
      tempDir,
      `const { readFileSync, writeFileSync } = process.getBuiltinModule("node:fs"); if (args.includes("list")) { console.log(JSON.stringify([{ name: "Suspended VM", status: readFileSync(${JSON.stringify(statePath)}, "utf8") }])); process.exit(0); } if (args.includes("resume")) { writeFileSync(${JSON.stringify(statePath)}, "running"); process.exit(0); } process.exit(1);`,
    );
    const sleepPath = join(tempDir, "sleep");
    writeFileSync(sleepPath, "#!/usr/bin/env bash\nexit 0\n");
    chmodSync(sleepPath, 0o755);

    withEnv(fakePrlctlEnv(tempDir), () => ensureVmRunning("Suspended VM"));
    expect(readFileSync(statePath, "utf8")).toBe("running");
  });
});
