import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  readGitCommitEnv,
  readPositiveIntEnv,
  resolveParallelsModelTimeoutSeconds,
} from "../../scripts/e2e/parallels/common.ts";
import { parseArgs as parseLinuxSmokeArgs } from "../../scripts/e2e/parallels/linux-smoke.ts";
import { parseArgs as parseMacosSmokeArgs } from "../../scripts/e2e/parallels/macos-smoke.ts";
import { parseArgs as parseNpmUpdateSmokeArgs } from "../../scripts/e2e/parallels/npm-update-smoke.ts";
import { parseArgs as parseWindowsSmokeArgs } from "../../scripts/e2e/parallels/windows-smoke.ts";
import { withEnv } from "../../src/test-utils/env.js";
import { expectFatalError } from "./parallels-smoke-command.test-support.js";
import { WRAPPERS, TS_PATHS, TS_SOURCE } from "./parallels-smoke-source.test-support.js";

const { linux, macos, npmUpdate, packageArtifact, windows } = TS_SOURCE;

describe("Parallels smoke arguments", () => {
  it("keeps the public shell entrypoints as thin TypeScript launchers", () => {
    for (const [platform, wrapperPath] of Object.entries(WRAPPERS)) {
      const wrapper = readFileSync(wrapperPath, "utf8");
      const scriptPath = platform === "npmUpdate" ? TS_PATHS.npmUpdate : TS_PATHS.linux;

      expect(wrapper, wrapperPath).toContain('cd "$ROOT_DIR"');
      expect(wrapper, wrapperPath).toContain(`exec node --import tsx ${scriptPath}`);
      expect(wrapper, wrapperPath).not.toContain("pnpm exec tsx");
    }
  });

  it("accepts leading package-manager separators and still honors later terminators", () => {
    expect(parseLinuxSmokeArgs(["--", "--mode", "upgrade"]).mode).toBe("upgrade");
    expect(parseLinuxSmokeArgs(["--mode", "fresh", "--", "--mode", "upgrade"]).mode).toBe("fresh");
    expect(parseMacosSmokeArgs(["--", "--mode", "upgrade"]).mode).toBe("upgrade");
    expect(parseMacosSmokeArgs(["--mode", "fresh", "--", "--mode", "upgrade"]).mode).toBe("fresh");
    expect(parseMacosSmokeArgs([]).vmNameExplicit).toBe(false);
    expect(parseMacosSmokeArgs(["--vm", "macOS"]).vmNameExplicit).toBe(true);
    expect(parseMacosSmokeArgs(["--host-port", "65535"]).hostPort).toBe(65535);
    expect(parseLinuxSmokeArgs(["--host-port", "65535"]).hostPort).toBe(65535);
    expect(parseWindowsSmokeArgs(["--host-port", "65535"]).hostPort).toBe(65535);
    expect(parseWindowsSmokeArgs([]).snapshotHint).toBe("pre-openclaw-native-e2e-");
    for (const parseArgs of [parseMacosSmokeArgs, parseLinuxSmokeArgs, parseWindowsSmokeArgs]) {
      expect(parseArgs(["--npm-registry", "http://192.0.2.2:48123"]).npmRegistry).toBe(
        "http://192.0.2.2:48123",
      );
    }
    expect(parseNpmUpdateSmokeArgs(["--", "--package-spec", "openclaw@2026.5.1"]).packageSpec).toBe(
      "openclaw@2026.5.1",
    );
    expect(
      parseNpmUpdateSmokeArgs([
        "--package-spec",
        "openclaw@2026.5.1",
        "--",
        "--package-spec",
        "openclaw@latest",
      ]).packageSpec,
    ).toBe("openclaw@2026.5.1");
    expect(parseNpmUpdateSmokeArgs(["--macos-vm", "macOS"]).macosVm).toBe("macOS");
    expect(parseWindowsSmokeArgs(["--", "--upgrade-from-packed-main"]).upgradeFromPackedMain).toBe(
      true,
    );
    expect(
      parseWindowsSmokeArgs(["--mode", "fresh", "--", "--upgrade-from-packed-main"])
        .upgradeFromPackedMain,
    ).toBe(false);
  });

  it("rejects short flags as Parallels smoke option values", () => {
    const cases = [
      [parseLinuxSmokeArgs, "--mode", "-h"],
      [parseMacosSmokeArgs, "--vm", "-h"],
      [parseWindowsSmokeArgs, "--model", "-h"],
      [parseNpmUpdateSmokeArgs, "--target-tarball", "-h"],
    ] as const;
    for (const [parseArgs, flag, value] of cases) {
      expectFatalError(() => parseArgs([flag, value]), `${flag} requires a value`);
    }
  });

  it("rejects inherited object keys as unknown Parallels smoke arguments", () => {
    for (const parseArgs of [parseMacosSmokeArgs, parseLinuxSmokeArgs, parseWindowsSmokeArgs]) {
      for (const arg of ["constructor", "toString"]) {
        expectFatalError(() => parseArgs([arg, "ignored"]), `unknown arg: ${arg}`);
      }
    }
  });

  it("rejects loose Parallels numeric limits before starting smoke lanes", () => {
    expect(
      withEnv({ OPENCLAW_PARALLELS_MODEL_TIMEOUT_S: "1200" }, () =>
        resolveParallelsModelTimeoutSeconds("linux"),
      ),
    ).toBe(1200);
    expect(
      withEnv({ OPENCLAW_PARALLELS_NUMERIC_TEST: " 42 " }, () =>
        readPositiveIntEnv("OPENCLAW_PARALLELS_NUMERIC_TEST", 7),
      ),
    ).toBe(42);
    expect(
      withEnv({ OPENCLAW_PARALLELS_DEV_TARGET_REF: ` ${"A".repeat(40)} ` }, () =>
        readGitCommitEnv("OPENCLAW_PARALLELS_DEV_TARGET_REF"),
      ),
    ).toBe("a".repeat(40));
    expect(
      withEnv({ OPENCLAW_PARALLELS_DEV_TARGET_REF: " " }, () =>
        readGitCommitEnv("OPENCLAW_PARALLELS_DEV_TARGET_REF"),
      ),
    ).toBeUndefined();

    expectFatalError(
      () =>
        withEnv({ OPENCLAW_PARALLELS_MACOS_MODEL_TIMEOUT_S: "1800s" }, () =>
          resolveParallelsModelTimeoutSeconds("macos"),
        ),
      "invalid OPENCLAW_PARALLELS_MACOS_MODEL_TIMEOUT_S: 1800s",
    );
    for (const [parseArgs, value] of [
      [parseMacosSmokeArgs, "18425x"],
      [parseLinuxSmokeArgs, "1e4"],
      [parseWindowsSmokeArgs, "0x4800"],
    ] as const) {
      expectFatalError(() => parseArgs(["--host-port", value]), `invalid --host-port: ${value}`);
    }
    for (const parseArgs of [parseMacosSmokeArgs, parseLinuxSmokeArgs, parseWindowsSmokeArgs]) {
      expectFatalError(() => parseArgs(["--host-port", "65536"]), "invalid --host-port: 65536");
    }
    for (const [name, value, fallback] of [
      ["OPENCLAW_PARALLELS_LINUX_AGENT_TIMEOUT_S", "1e3", 1500],
      ["OPENCLAW_PARALLELS_WINDOWS_AGENT_TIMEOUT_S", "2700s", 2700],
      ["OPENCLAW_PARALLELS_WINDOWS_UPDATE_TIMEOUT_S", "12.5", 7200],
    ] as const) {
      expectFatalError(
        () => withEnv({ [name]: value }, () => readPositiveIntEnv(name, fallback)),
        `invalid ${name}: ${value}`,
      );
    }
    expectFatalError(
      () =>
        withEnv({ OPENCLAW_PARALLELS_DEV_TARGET_REF: "main" }, () =>
          readGitCommitEnv("OPENCLAW_PARALLELS_DEV_TARGET_REF"),
        ),
      "invalid OPENCLAW_PARALLELS_DEV_TARGET_REF: expected a full 40-character commit SHA",
    );
    expectFatalError(
      () => parseNpmUpdateSmokeArgs(["--platform", "macos,macos"]),
      "duplicate --platform entry: macos",
    );

    expect(macos).toContain(
      'this.updateDevTimeoutSeconds = readPositiveIntEnv(\n      "OPENCLAW_PARALLELS_MACOS_UPDATE_DEV_TIMEOUT_S"',
    );
    expect(linux).toContain('readPositiveIntEnv(\n    "OPENCLAW_PARALLELS_LINUX_AGENT_TIMEOUT_S"');
    expect(windows).toContain(
      'readPositiveIntEnv(\n    "OPENCLAW_PARALLELS_WINDOWS_UPDATE_TIMEOUT_S"',
    );
    expect(packageArtifact).toContain(
      'readPositiveIntEnv("OPENCLAW_PARALLELS_PACKAGE_LOCK_TIMEOUT_MS", 30 * 60_000)',
    );
    expect(npmUpdate).toContain(
      'readPositiveIntEnv("OPENCLAW_PARALLELS_NPM_UPDATE_TIMEOUT_S", 2700)',
    );
  });
});
