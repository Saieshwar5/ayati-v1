#!/usr/bin/env node
// Generic core release finalization retained from the removed desktop channel owner.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import {
  classifyReleaseTrain,
  compareReleaseVersions,
  parseReleaseVersion,
} from "./lib/release-version.mjs";
import {
  verifyReleaseToolingIdentity,
  verifyReleaseWorkflowRun,
} from "./release-tooling-identity.mjs";
const REPOSITORY = "openclaw/openclaw";
const METADATA_LIMIT = 1024 * 1024;
const SHA = /^[0-9a-f]{40}$/u;
let commandOverride;
function command(binary, args, timeout = 60_000) {
  if (commandOverride) {
    return commandOverride(binary, args, timeout);
  }
  return execFileSync(binary, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    timeout,
    killSignal: "SIGKILL",
    maxBuffer: 2 * METADATA_LIMIT,
    env: {
      ...process.env,
      GH_PROMPT_DISABLED: "1",
      TAURI_SIGNING_PRIVATE_KEY: "",
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "",
    },
  });
}

function authorizeWrite(options) {
  const runGh = commandOverride ? (args) => command("gh", args) : undefined;
  if (options.tag.includes("-alpha.") || options["workflow-ref"].includes("tideclaw/alpha/")) {
    throw new Error("Alpha releases are retired; use a beta prerelease instead.");
  }
  verifyReleaseToolingIdentity({
    repository: REPOSITORY,
    workflowRef: options["workflow-ref"],
    workflowFullRef: options["workflow-full-ref"],
    workflowSha: options["tooling-sha"],
    ...(runGh ? { runGh } : {}),
    releasePublishRunId: options["release-publish-run-id"],
    releasePublishRunAttempt: options["release-publish-run-attempt"],
    releasePublishRef: options["release-publish-ref"],
    releasePublishFullRef: options["release-publish-full-ref"],
    releasePublishParentStatePolicy: "active-or-success",
  });
  verifyReleaseWorkflowRun({
    repository: REPOSITORY,
    workflowRef: options["workflow-ref"],
    workflowFullRef: options["workflow-full-ref"],
    workflowSha: options["tooling-sha"],
    runId: process.env.GITHUB_RUN_ID,
    runAttempt: process.env.GITHUB_RUN_ATTEMPT,
    workflowPath: ".github/workflows/openclaw-release-publish.yml",
    workflowEvent: "workflow_dispatch",
    runStatePolicy: "active",
    ...(runGh ? { runGh } : {}),
  });
}

function regularVersion(version) {
  const parsed = parseReleaseVersion(version);
  assert(
    parsed && parsed.version === version && classifyReleaseTrain(parsed) === "stable",
    `Not a canonical regular release: ${version}`,
  );
  return version;
}

function releaseVersion(tag) {
  assert(typeof tag === "string" && tag.startsWith("v"), "Expected a versioned release tag");
  return regularVersion(tag.slice(1));
}

function identity(release) {
  return [release.id, release.tag_name, release.draft, release.prerelease];
}

class GitHub {
  constructor(options) {
    this.options = options;
  }

  authorize() {
    authorizeWrite(this.options);
  }

  api(endpoint, missing = false) {
    try {
      return JSON.parse(command("gh", ["api", `repos/${REPOSITORY}/${endpoint}`]));
    } catch (error) {
      if (missing && /\bHTTP 404\b/u.test(String(error.stderr))) {
        return null;
      }
      throw error;
    }
  }

  release(tag, missing = false) {
    const release = this.api(`releases/tags/${encodeURIComponent(tag)}`, missing);
    if (!release) {
      return null;
    }
    assert(
      Number.isSafeInteger(release.id) &&
        release.id > 0 &&
        release.tag_name === tag &&
        typeof release.draft === "boolean" &&
        typeof release.prerelease === "boolean",
      "Release identity is invalid",
    );
    const assets = [];
    for (let page = 1; page <= 5; page++) {
      const rows = this.api(`releases/${release.id}/assets?per_page=100&page=${page}`);
      assert(Array.isArray(rows) && rows.length <= 100, "Invalid asset inventory");
      assets.push(...rows);
      if (rows.length < 100) {
        return { ...release, assets };
      }
    }
    throw new Error("Release asset inventory exceeds the publication bound");
  }

  latest() {
    const latest = this.api("releases/latest", true);
    if (!latest) {
      return null;
    }
    const release = this.release(latest.tag_name);
    assert.equal(release.id, latest.id, "Latest release changed during observation");
    return release;
  }

  source(tag, missing = false) {
    const ref = this.api(`git/ref/tags/${encodeURIComponent(tag)}`, missing);
    if (!ref) {
      return null;
    }
    let object = ref.object;
    for (let depth = 0; depth < 5; depth++) {
      assert(object && SHA.test(object.sha), "Invalid release tag object");
      if (object.type === "commit") {
        return object.sha;
      }
      assert.equal(object.type, "tag", "Release tag must resolve to a commit");
      object = this.api(`git/tags/${object.sha}`).object;
    }
    throw new Error("Release tag nesting exceeds the publication bound");
  }
}

function finalizeCore(github, options) {
  const version = options.tag.slice(1);
  const parsed = parseReleaseVersion(version);
  if (parsed?.channel === "alpha") {
    throw new Error("Alpha releases are retired; use a beta prerelease instead.");
  }
  const train = parsed && classifyReleaseTrain(parsed);
  assert(
    parsed && parsed.version === version && ["stable", "beta", "extended-stable"].includes(train),
    "Unsupported core GitHub release train",
  );
  assert(["true", "false"].includes(options.latest), "Expected explicit core latest intent");
  assert(
    train !== "extended-stable" || options.latest === "false",
    "Extended-stable releases cannot become core latest",
  );
  const prerelease = parsed.channel !== "stable";
  assert(!prerelease || options.latest === "false", "Prereleases cannot become core latest");
  github.authorize();
  // gh release view also discovers drafts by their pending tag; REST's tag route does not.
  const discovered = JSON.parse(
    command("gh", [
      "release",
      "view",
      options.tag,
      "--repo",
      REPOSITORY,
      "--json",
      "databaseId,tagName,isDraft,isPrerelease",
    ]),
  );
  const release = {
    id: discovered.databaseId,
    tag_name: discovered.tagName,
    draft: discovered.isDraft,
    prerelease: discovered.isPrerelease,
  };
  assert(
    Number.isSafeInteger(release.id) &&
      release.id > 0 &&
      release.tag_name === options.tag &&
      typeof release.draft === "boolean" &&
      typeof release.prerelease === "boolean",
    "Selected core release identity is invalid",
  );
  assert.equal(release.prerelease, prerelease, "Selected core prerelease classification changed");
  assert.equal(github.source(options.tag), options["source-sha"], "Selected core tag moved");
  const latest = github.latest();
  const latestSource = latest ? github.source(latest.tag_name) : null;
  let makeLatest = options.latest === "true";
  if (latest) {
    assert(!latest.draft && !latest.prerelease, "Core latest is not a public regular release");
    const latestVersion = releaseVersion(latest.tag_name);
    if (makeLatest && compareReleaseVersions(version, latestVersion) < 0) {
      makeLatest = false;
    }
    assert(
      makeLatest || latest.id !== release.id,
      "Non-latest finalization must not demote the current latest release",
    );
  }
  const verifyLatest = () => {
    const fresh = github.latest();
    assert.deepEqual(
      fresh ? identity(fresh) : null,
      latest ? identity(latest) : null,
      "Core latest changed before finalization",
    );
    if (latest) {
      assert.equal(github.source(latest.tag_name), latestSource, "Core latest tag moved");
    }
  };
  assert.deepEqual(
    identity(github.api(`releases/${release.id}`)),
    identity(release),
    "Release identity changed before write",
  );
  assert.equal(github.source(options.tag), options["source-sha"], "Selected core tag moved");
  verifyLatest();
  github.authorize();
  // Address the admitted release ID, never a re-resolved replacement with the same tag.
  command("gh", [
    "api",
    "--method",
    "PATCH",
    `repos/${REPOSITORY}/releases/${release.id}`,
    "--field",
    "draft=false",
    "--raw-field",
    `make_latest=${makeLatest}`,
  ]);
  const published = github.release(options.tag);
  assert.deepEqual(
    identity(published),
    [release.id, options.tag, false, prerelease],
    "Finalized core release identity mismatch",
  );
  assert.equal(github.source(options.tag), options["source-sha"], "Finalized core tag moved");
  const actualLatest = github.latest();
  const expectedLatest = makeLatest ? published : latest;
  assert.deepEqual(
    actualLatest ? identity(actualLatest) : null,
    expectedLatest ? identity(expectedLatest) : null,
    "Core latest readback mismatch",
  );
  if (actualLatest) {
    assert.equal(
      github.source(actualLatest.tag_name),
      makeLatest ? options["source-sha"] : latestSource,
      "Core latest source readback mismatch",
    );
  }
  return {
    state: "finalized",
    tag: options.tag,
    releaseId: release.id,
    sourceSha: options["source-sha"],
    madeLatest: makeLatest,
    latestReleaseId: actualLatest?.id ?? null,
    latestTag: actualLatest?.tag_name ?? null,
  };
}

function main(args) {
  const { values, positionals } = parseArgs({
    args,
    allowPositionals: true,
    options: Object.fromEntries(
      [
        "tag",
        "source-sha",
        "tooling-sha",
        "workflow-ref",
        "workflow-full-ref",
        "release-publish-run-id",
        "release-publish-run-attempt",
        "release-publish-ref",
        "release-publish-full-ref",
        "latest",
      ].map((name) => [name, { type: "string" }]),
    ),
  });
  assert(
    positionals.length === 1 && positionals[0] === "finalize-core",
    "Usage: release-core-finalize.mjs finalize-core --tag TAG --source-sha SHA [identity options]",
  );
  assert(SHA.test(values["source-sha"]), "Expected the approved source SHA");
  assert(
    SHA.test(values["tooling-sha"]) && values["workflow-ref"] && values["workflow-full-ref"],
    "Missing publication tooling identity",
  );
  for (const key of ["release-publish-run-id", "release-publish-run-attempt"]) {
    assert(
      /^[1-9][0-9]*$/u.test(values[key]) && Number.isSafeInteger(Number(values[key])),
      `Missing or invalid ${key}`,
    );
  }
  assert(
    values["release-publish-ref"] && values["release-publish-full-ref"],
    "Missing core publisher identity",
  );
  assert(typeof values.tag === "string" && values.tag.startsWith("v"), "Expected a release tag");
  return finalizeCore(new GitHub(values), values);
}

export function runReleaseCoreFinalization(args, options = {}) {
  const previous = commandOverride;
  commandOverride = options.runCommand;
  try {
    return main(args);
  } finally {
    commandOverride = previous;
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  try {
    process.stdout.write(
      `${JSON.stringify(runReleaseCoreFinalization(process.argv.slice(2)), null, 2)}\n`,
    );
  } catch (error) {
    console.error(`Release publication incomplete; reconcile before retry: ${error.message}`);
    process.exitCode = 1;
  }
}
