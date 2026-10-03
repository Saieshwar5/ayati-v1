import { chmodSync, copyFileSync, writeFileSync } from "node:fs";
import { delimiter, join } from "node:path";
import { pathToFileURL } from "node:url";
import { expect, vi } from "vitest";
import { resolveTestNodeExecPath } from "../../src/test-utils/node-process.js";

export const testNodeExecPath = resolveTestNodeExecPath();

export function expectFatalError(runTest: () => unknown, message: string): void {
  const stderr = vi.spyOn(process.stderr, "write").mockImplementation(() => true);
  const exit = vi.spyOn(process, "exit").mockImplementation((code) => {
    throw new Error(`process.exit(${code})`);
  });
  try {
    expect(runTest).toThrow("process.exit(1)");
    expect(stderr).toHaveBeenLastCalledWith(`error: ${message}\n`);
  } finally {
    exit.mockRestore();
    stderr.mockRestore();
  }
}

export function fakePrlctlEnv(tempDir: string): Record<string, string> {
  const pathValue = `${tempDir}${delimiter}${process.env.Path ?? process.env.PATH ?? ""}`;
  const fakeBootstrap = pathToFileURL(join(tempDir, "prlctl-bootstrap.mjs")).href;
  const nodeOptions = [process.env.NODE_OPTIONS, `--import=${fakeBootstrap}`]
    .filter(Boolean)
    .join(" ");
  return { NODE_OPTIONS: nodeOptions, PATH: pathValue, Path: pathValue };
}

function writeFakePrlctl(tempDir: string, posixScript: string, windowsBootstrap: string): void {
  const prlctlPath = join(tempDir, "prlctl");
  writeFileSync(prlctlPath, posixScript);
  chmodSync(prlctlPath, 0o755);
  if (process.platform === "win32") {
    copyFileSync(testNodeExecPath, join(tempDir, "prlctl.exe"));
  }
  writeFileSync(join(tempDir, "prlctl-bootstrap.mjs"), windowsBootstrap);
}

export function writeNodeFakePrlctl(tempDir: string, body: string): void {
  const program = `const args = process.argv.slice(1);\n${body}`;
  writeFakePrlctl(
    tempDir,
    `#!/usr/bin/env node\n${program}\n`,
    `import { basename } from "node:path"; if ([process.argv0, process.execPath].some((value) => basename(value).toLowerCase() === "prlctl.exe")) { ${program} }`,
  );
}
