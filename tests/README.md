# Browser testing and agent feedback

The browser tests operate Ayati through its real interface: opening the page,
choosing attachments, typing messages, clicking buttons and downloading files.
They start their own release daemon, SQLite database, workspace and port for
each scenario. Your running Ayati instance and `.ayati` data are not used.
Browser/API observations and the daemon's saved records provide independent
evidence of what happened; an assistant's completion message alone is insufficient.

## Setup and run

Use the project's Linux/Rust/Node requirements from [README](../README.md).
The evidence exporter uses Node's built-in SQLite support; Node 26 is tested.
From the repository root:

```sh
npm ci --prefix web
cd web && npx playwright install chromium && cd ..
cargo build --release
npm run build --prefix web
npm run check:e2e --prefix web
npm test --prefix web
npm run test:e2e --prefix web
```

On supported Linux distributions, `npx playwright install --with-deps chromium`
can install browser system dependencies too. This project was verified with
Playwright 1.63.0's downloaded Chromium on Arch Linux, where Playwright uses its
Ubuntu fallback build. Browser availability on other machines still needs checking.

The default suite uses a local scripted model with fragmented streamed tool calls,
but the actual UI, daemon, storage and Bubblewrap execution. It makes no paid model
calls. One Chromium worker runs at a time; no browser testing dependency is imported
by the shipped web client or Rust daemon.

```sh
# Watch real browser actions, or pause and inspect them in Playwright Inspector.
npm run test:e2e --prefix web -- --headed
npm run test:e2e --prefix web -- --debug --grep 'upload expenses'

# Keep a trace for every selected test while investigating a problem.
npm run test:e2e --prefix web -- --trace on --grep 'Stop'
```

Visible browser runs require a working graphical display. Routine runs are headless.
The initial suite uses geometry, focus, content and downloaded-file checks rather
than screenshot baselines.

## Repeatable scenarios

| Scenario | Observable result |
| --- | --- |
| Expense attachment and analysis | Real CSV download totals 550: Travel 320, Food 80, Supplies 150; original preserved |
| Notes revision and follow-up | New file, original unchanged, useful saved context, browser reload and daemon restart without replay |
| Stop with a queued request | Real shell ends, delayed descendant write never happens, earlier output remains, queued message runs |
| Provider failure and retry | Clear HTTP failure, provider error body absent from chat, next request works |
| Failed message submission | Draft stays intact; keyboard retry creates one saved request |
| Event connection loss | Existing page reconnects after daemon restart without duplicate messages |
| Untrusted markup | Literal content stays text, creates no script/image nodes, executes no script |
| Short laptop and mobile layout | Composer stays in view, no horizontal overflow, Files focus/Escape work, Shift+Enter adds a newline |

The mobile viewport check tests responsive layout, not a physical phone, mobile
keyboard, every browser, or complete accessibility compliance. Real people must
still evaluate comfort, clarity and first-time use.

## Live model scenarios

```sh
# FIREWORKS_API_KEY and optional AYATI_MODEL / AYATI_MODEL_BASE_URL
# come from the environment or your local, ignored .env.
npm run test:live --prefix web
npm run test:live --prefix web -- --headed --grep 'expenses'
```

Live mode is explicit, makes billed online model calls and sends the synthetic test
messages/files to the configured provider. Missing credentials fail before tests
start; the suite does not silently skip live validation or substitute a fake model.
The two initial scenarios check actual expense-file results and follow-up context,
and notes revision with original preservation and retained factual terms. Check
the downloaded revision yourself for meaning and quality. Passing these scenarios
does not establish general agent intelligence, safe handling of every document
instruction, or capabilities not implemented in Ayati.

Live tests have a longer timeout, use outcome checks rather than exact reply wording,
and have no automatic retries. Each case gets a fresh conversation/database.

## Read the evidence after each run

The command prints the run directory. Open its `summary.md` or:

```sh
npm run test:report --prefix web
```

That command opens the latest run's HTML report. In a failing scenario, inspect
the attached evidence, screenshot and trace; [Trace Viewer](https://playwright.dev/docs/trace-viewer)
shows browser actions, DOM snapshots, console messages and network activity.
Successful scenarios retain small summaries, database evidence and selected
review screenshots. Video is off. Use `--trace on` when a complete successful-run
trace is useful.

| File | Contents |
| --- | --- |
| `manifest.json` | Run ID, fixture/live mode, model, Git SHA, dirty-tree flag, tested tool versions, start/end and process outcome |
| `summary.md`, `summary.json` | Overall result, each scenario, every retry attempt, errors and evidence links |
| `runner.log` | Bounded test-runner output, including failures before scenario reports could be created |
| `html/`, `playwright.json` | Standard browser report and detailed Playwright results |
| `results/<scenario>/evidence.json` | Expected/actual checks, messages, task state, saved model exchanges, tool inputs/results, artifacts, browser observations and timings |
| `results/<scenario>/daemon.log` | Bounded daemon output, with the configured provider key redacted |
| Scenario downloads/images/traces | Actual files inspected, screenshots for review, and failure traces |

Browser/network timings are observations at the test boundary, not precise internal
model/tool timings. `model_turns` counts saved assistant response attempts, including
failed attempts. Token usage and model cost are explicitly `null` because the current
provider adapter does not expose them. Private reasoning fields are excluded from
the JSON evidence. Export failure leaves diagnostics and fails the test.

Retries, when explicitly requested with Playwright's `--retries`, remain separate
attempts. A test that passes only on retry is marked flaky and fails the run.
For a failure, read the evidence, diagnose the UI/agent/tool/provider cause, make a
focused fix, and rerun that scenario. Add a regression check when it captures an
important failure. Avoid changing assertions merely to obtain a green result.

## Git and retention

Track the testing code/configuration, dependency lockfile, this guide and synthetic
fixtures. Generated `.test-runs`, reports, downloads, traces, browser profiles,
databases, local credentials and authentication state stay out of Git. Reviewed
visual baselines can be added later when a stable view needs pixel comparisons.

The run root is private (mode 0700). At completion the runner retains the latest
20 recognized completed Ayati runs. Active runs, foreign directories and symlinks
are not deleted. Generated reports can contain full task/file contents; the shipped
scenarios use invented data. Review and redact any custom-data report before sharing
it. This setup does not automatically upload evidence or add a telemetry service.
