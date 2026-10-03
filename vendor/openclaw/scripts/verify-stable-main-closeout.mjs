#!/usr/bin/env node
// Ayati retains generic evidence verification; installed desktop release closeout is retired.
import { readFileSync } from "node:fs";
import { basename, resolve } from "node:path";
import { verifyReleaseEvidenceChecksum } from "./lib/stable-release-closeout.mjs";

try {
  if (process.argv[2] !== "verify-checksum") {
    throw new Error(
      "Desktop release closeout is disabled in Ayati; use the Ayati release process.",
    );
  }
  if (process.argv.length !== 4) {
    throw new Error("usage: verify-stable-main-closeout.mjs verify-checksum <evidence-file>");
  }
  const path = resolve(process.argv[3]);
  verifyReleaseEvidenceChecksum({
    assetName: basename(path),
    assetBytes: readFileSync(path),
    checksum: readFileSync(`${path}.sha256`, "utf8"),
  });
  console.log(`release evidence checksum verified: ${basename(path)}`);
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
