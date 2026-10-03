import { vi } from "vitest";
import { run } from "../../scripts/e2e/parallels/common.ts";
import { runPosixBackgroundShell } from "../../scripts/e2e/parallels/guest-transports.ts";

type FakeCommandResult = { status: number; stderr: string; stdout: string };

type FakePosixBackgroundOptions = {
  done?: FakeCommandResult;
  exit?: FakeCommandResult[];
  launch?: FakeCommandResult;
  pid?: FakeCommandResult;
  tail?: () => FakeCommandResult;
};

export function fakeResult(status = 0, stdout = "", stderr = ""): FakeCommandResult {
  return { status, stderr, stdout };
}

function createPosixBackgroundCommandFake(options: FakePosixBackgroundOptions): {
  runCommand: ReturnType<typeof vi.fn>;
  state: { cleanupPayload: string; cleanupRun: boolean; exitRead: boolean };
} {
  const state = { cleanupPayload: "", cleanupRun: false, exitRead: false };
  const exits = [...(options.exit ?? [])];
  const runCommand = vi.fn(
    (_command: string, args: string[], runOptions?: { input?: string }): FakeCommandResult => {
      if (args[0] === "/bin/dd" && args[1]?.includes("/cleanup.sh")) {
        state.cleanupPayload = runOptions?.input ?? "";
      }
      if (args[0] === "node" && args[1]?.endsWith("/launcher.mjs")) {
        return options.launch ?? fakeResult(0, "started\n");
      }
      if (args[0] === "/bin/test" && args.at(-1)?.endsWith("/pid")) {
        return options.pid ?? fakeResult();
      }
      if (args[0] === "/bin/test" && args.at(-1)?.endsWith("/done")) {
        return options.done ?? fakeResult();
      }
      if (args[0] === "/usr/bin/tail") {
        return options.tail?.() ?? fakeResult();
      }
      if (args[0] === "/bin/cat" && args.at(-1)?.endsWith("/exit")) {
        state.exitRead = true;
        return exits.shift() ?? fakeResult();
      }
      if (args[0] === "/bin/bash" && args[1]?.endsWith("/cleanup.sh")) {
        state.cleanupRun = true;
      }
      return fakeResult(0, "started\n");
    },
  );
  return { runCommand, state };
}

export function runFakePosixBackground(
  fakeOptions: FakePosixBackgroundOptions,
  options: { append?: (chunk: unknown) => void; script: string; timeoutMs?: number },
) {
  const fake = createPosixBackgroundCommandFake(fakeOptions);
  return {
    ...fake,
    result: runPosixBackgroundShell({
      ...options,
      label: "macos update",
      pollIntervalMs: 1,
      runCommand: fake.runCommand as unknown as typeof run,
      timeoutMs: options.timeoutMs ?? 5_000,
      transportArgs: (args) => args,
    }),
  };
}
