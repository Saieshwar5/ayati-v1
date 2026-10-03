import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  modelProviderConfigBatchJson,
  parseProvider,
  resolveParallelsModelTimeoutSeconds,
  resolveProviderAuth as resolveProviderAuthDirect,
  resolveWindowsProviderAuth,
} from "../../scripts/e2e/parallels/common.ts";
import {
  posixCodexPlatformPackageRepairFunction,
  windowsProviderOnlyPluginIsolationScript,
  windowsCodexPlatformPackageRepairFunction,
} from "../../scripts/e2e/parallels/plugin-isolation.ts";
import {
  resolveParallelsProviderAuth,
  runParallelsPrerequisiteEval,
} from "../../scripts/e2e/parallels/provider-auth-prerequisite.mjs";
import { withEnv } from "../../src/test-utils/env.js";
import { expectFatalError } from "./parallels-smoke-command.test-support.js";
import {
  TS_PATHS,
  TS_SOURCE,
  OS_TS_PATHS,
  PROVIDER_AUTH_PREREQUISITE_SOURCE,
} from "./parallels-smoke-source.test-support.js";

const { linux, macos, windows } = TS_SOURCE;

describe("Parallels smoke model", () => {
  it("keeps provider auth and model defaults in the shared helper", () => {
    expect(PROVIDER_AUTH_PREREQUISITE_SOURCE).toContain("OPENCLAW_PARALLELS_WINDOWS_OPENAI_MODEL");
    expect(PROVIDER_AUTH_PREREQUISITE_SOURCE).toContain("openai/gpt-5.6-luna");
    expect(PROVIDER_AUTH_PREREQUISITE_SOURCE).toContain("tokenProvider: input.provider");

    for (const scriptPath of [...OS_TS_PATHS, TS_PATHS.npmUpdate]) {
      const script = readFileSync(scriptPath, "utf8");

      expect(script, scriptPath).toMatch(/resolve(?:Windows)?ProviderAuth/u);
      expect(script, scriptPath).toContain("--model <provider/model>");
      expect(script, scriptPath).toContain("modelId");
    }

    for (const scriptPath of [TS_PATHS.linux, TS_PATHS.macos]) {
      expect(readFileSync(scriptPath, "utf8")).toContain(
        '...(this.auth.tokenProvider ? ["--token-provider", this.auth.tokenProvider] : [])',
      );
    }
    expect(windows).toContain("tokenProviderArg");
  });

  it("repairs only the exact missing Codex platform package failure with a fresh npm cache", () => {
    const posixRepair = posixCodexPlatformPackageRepairFunction();
    const windowsRepair = windowsCodexPlatformPackageRepairFunction();

    for (const repair of [posixRepair, windowsRepair]) {
      expect(repair).toContain("Missing optional dependency @openai/codex-");
      expect(repair).toContain("NPM_CONFIG_CACHE");
      expect(repair).toContain("--ignore-scripts");
      expect(repair).toContain("codex-platform-repair: managed npm install completed");
    }
    expect(posixRepair).toContain("repair_missing_codex_platform_package");
    expect(windowsRepair).toContain("Repair-MissingCodexPlatformPackage");
  });

  it("keeps Windows provider-only plugin isolation temp scripts per run", () => {
    const script = windowsProviderOnlyPluginIsolationScript({
      fallbackPluginId: "openai",
      modelId: "openai/gpt-5.6-luna",
    });

    expect(script).toContain("[guid]::NewGuid().ToString('N')");
    expect(script).toContain("openclaw-parallels-plugin-isolation-");
    expect(script).not.toContain("'openclaw-parallels-plugin-isolation.cjs'");
    expect(script).toContain("try {");
    expect(script).toContain("} finally {");
    expect(script).toContain(
      "Remove-Item $isolationScriptPath -Force -ErrorAction SilentlyContinue",
    );
    expect(script).toContain("Remove-Item Env:OPENCLAW_PARALLELS_PLUGIN_ISOLATION");
  });

  it("writes full model ids as config map keys in provider batches", () => {
    const batch = JSON.parse(modelProviderConfigBatchJson("openai/gpt-5.5", "windows")) as Array<{
      path: string;
      value: unknown;
    }>;

    expect(batch.map((entry) => entry.path)).toContain('agents.defaults.models["openai/gpt-5.5"]');
    expect(JSON.stringify(batch)).not.toContain("agentRuntime");
  });

  it("resolves provider defaults and explicit model overrides", () => {
    expect(
      resolveParallelsProviderAuth({ provider: "openai" }, { OPENAI_API_KEY: "sk-openai" }),
    ).toMatchObject({
      auth: { apiKeyEnv: "OPENAI_API_KEY", modelId: "openai/gpt-5.6-luna" },
      reason: null,
      status: "ready",
    });
    expect(
      resolveParallelsProviderAuth(
        {
          apiKeyEnv: "CUSTOM_ANTHROPIC_KEY",
          provider: "anthropic",
        },
        {
          CUSTOM_ANTHROPIC_KEY: "sk-anthropic",
          OPENCLAW_PARALLELS_ANTHROPIC_MODEL: "  anthropic/custom  ",
        },
      ),
    ).toMatchObject({
      auth: { apiKeyEnv: "CUSTOM_ANTHROPIC_KEY", modelId: "  anthropic/custom  " },
      reason: null,
      status: "ready",
    });
    const inheritedCredentials = Object.create(null) as Record<string, string>;
    for (const name of ["constructor", "toString", "__proto__"]) {
      Object.defineProperty(inheritedCredentials, name, { value: "inherited-secret" });
    }
    const inheritedEnv = Object.create(inheritedCredentials) as Record<string, string>;
    for (const apiKeyEnv of ["constructor", "toString", "__proto__"]) {
      expect(
        resolveParallelsProviderAuth({ apiKeyEnv, provider: "openai" }, inheritedEnv),
      ).toMatchObject({
        auth: { apiKeyValue: "" },
        reason: "credential_missing",
        status: "blocked",
      });
    }
    expect(
      withEnv({ OPENAI_API_KEY: "sk-openai" }, () =>
        resolveProviderAuthDirect({ provider: "openai" }),
      ),
    ).toEqual({
      apiKeyEnv: "OPENAI_API_KEY",
      apiKeyValue: "sk-openai",
      authChoice: "apiKey",
      authKeyFlag: "openai-api-key",
      modelId: "openai/gpt-5.6-luna",
      tokenProvider: "openai",
    });

    const windowsEnv = {
      OPENAI_API_KEY: "sk-openai",
      OPENCLAW_PARALLELS_OPENAI_MODEL: "openai/generic",
      OPENCLAW_PARALLELS_WINDOWS_OPENAI_MODEL: "openai/windows",
    };
    const frozenWindows = resolveParallelsProviderAuth(
      { platform: "windows", provider: "openai" },
      windowsEnv,
    );
    expect(frozenWindows).toMatchObject({
      auth: { modelId: "openai/windows" },
      reason: null,
      status: "ready",
    });
    expect(withEnv(windowsEnv, () => resolveWindowsProviderAuth({ provider: "openai" }))).toEqual(
      frozenWindows.auth,
    );
    expect(
      resolveParallelsProviderAuth(
        { modelId: "openai/explicit", platform: "windows", provider: "openai" },
        windowsEnv,
      ).auth.modelId,
    ).toBe("openai/explicit");
    const whitespaceEnv = { OPENAI_API_KEY: "sk-openai", OPENCLAW_PARALLELS_OPENAI_MODEL: "   " };
    expect([
      resolveParallelsProviderAuth({ platform: "windows", provider: "openai" }, whitespaceEnv).auth
        .modelId,
      withEnv(whitespaceEnv, () => resolveWindowsProviderAuth({ provider: "openai" }).modelId),
    ]).toEqual(["openai/gpt-5.6-luna", "openai/gpt-5.6-luna"]);

    expect(
      withEnv({ CUSTOM_ANTHROPIC_KEY: "sk-anthropic" }, () =>
        resolveProviderAuthDirect({
          apiKeyEnv: "CUSTOM_ANTHROPIC_KEY",
          modelId: "anthropic/custom",
          provider: "anthropic",
        }),
      ),
    ).toEqual({
      apiKeyEnv: "CUSTOM_ANTHROPIC_KEY",
      apiKeyValue: "sk-anthropic",
      authChoice: "apiKey",
      authKeyFlag: "anthropic-api-key",
      modelId: "anthropic/custom",
      tokenProvider: "anthropic",
    });
  });

  it("keeps prerequisite adapter failures sanitized and closed", () => {
    const cases = [
      ["--prerequisite-check", "--json", "--provider", "openai", "--provider", "anthropic"],
      ["--prerequisite-check", "--json", "--unknown"],
      ["--prerequisite-check", "--json", "--provider"],
      ["--prerequisite-check", "--json", "--provider", "unsupported-secret"],
      ["--prerequisite-check", "--json", "--provider", "constructor"],
      ["--prerequisite-check", "--json", "--platform", "private-platform"],
      ["--prerequisite-check", "--json", "--only", "windows,windows"],
      ["--prerequisite-check"],
    ];
    for (const args of cases) {
      const output: string[] = [];
      expect(runParallelsPrerequisiteEval(args, {}, { write: (value) => output.push(value) })).toBe(
        1,
      );
      expect(output).toEqual([
        '{"schema":"openclaw.parallels-prerequisite.v1","status":"blocked","reason":"invalid_arguments"}\n',
      ]);
      expect(output[0]).not.toContain("unsupported-secret");
    }

    const inheritedCredentials = Object.create(null) as Record<string, string>;
    for (const name of ["constructor", "toString", "__proto__"]) {
      Object.defineProperty(inheritedCredentials, name, { value: "inherited-secret" });
    }
    for (const apiKeyEnv of ["constructor", "toString", "__proto__"]) {
      const output: string[] = [];
      expect(
        runParallelsPrerequisiteEval(
          ["--prerequisite-check", "--json", "--api-key-env", apiKeyEnv],
          Object.create(inheritedCredentials) as Record<string, string>,
          { write: (value) => output.push(value) },
        ),
      ).toBe(1);
      expect(output).toEqual([
        '{"schema":"openclaw.parallels-prerequisite.v1","status":"blocked","reason":"credential_missing"}\n',
      ]);
    }

    const output: string[] = [];
    const env = { OPENAI_API_KEY: "sk-openai" };
    Object.defineProperty(env, "OPENCLAW_PARALLELS_WINDOWS_OPENAI_MODEL", {
      get: () => {
        throw new Error("sentinel-secret");
      },
    });
    const write = (value: string) => output.push(value);
    expect(
      [
        ["--prerequisite-check", "--json"],
        ["--prerequisite-check", "--json", "--platform", "windows"],
        ["--prerequisite-check", "--json", "--only", "linux"],
      ].map((args) => runParallelsPrerequisiteEval(args, env, { write })),
    ).toEqual([1, 1, 0]);
    expect(output).toEqual([
      '{"schema":"openclaw.parallels-prerequisite.v1","status":"blocked","reason":"internal_error"}\n',
      '{"schema":"openclaw.parallels-prerequisite.v1","status":"blocked","reason":"internal_error"}\n',
      '{"schema":"openclaw.parallels-prerequisite.v1","status":"ready","reason":null}\n',
    ]);
    expect(output.join("")).not.toContain("sentinel-secret");
  });

  it("rejects invalid providers and missing keys before touching guests", () => {
    expectFatalError(() => parseProvider("bogus"), "invalid --provider: bogus");
    expectFatalError(
      () =>
        withEnv({ PARALLELS_TEST_MISSING_KEY: "" }, () =>
          resolveProviderAuthDirect({
            apiKeyEnv: "PARALLELS_TEST_MISSING_KEY",
            provider: "openai",
          }),
        ),
      "PARALLELS_TEST_MISSING_KEY is required",
    );
  });

  it("gives GPT-5.6 Luna enough Parallels model time on slower desktop guests", () => {
    expect({
      linux: resolveParallelsModelTimeoutSeconds("linux"),
      macos: resolveParallelsModelTimeoutSeconds("macos"),
      windows: resolveParallelsModelTimeoutSeconds("windows"),
    }).toEqual({
      linux: 900,
      macos: 1800,
      windows: 1800,
    });
    expect(macos).toContain(
      'this.agentTimeoutSeconds = readPositiveIntEnv("OPENCLAW_PARALLELS_MACOS_AGENT_TIMEOUT_S", 2700)',
    );
    expect(macos).toContain("--timeout ${this.modelTimeoutSeconds}");
    expect(linux).toContain('--timeout ${resolveParallelsModelTimeoutSeconds("linux")}');
  });
});
