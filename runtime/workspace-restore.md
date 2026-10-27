# Local workspace backup and restore rehearsal

Implemented local rehearsal after personal Home panels and trusted conversation
context. This extends the earlier
company-simulation restore comparison to a current isolated workspace schema and
its recorded private chat files.

`tools/workspace_restore.py` accepts only a local `relay_simulation_*` database and an explicit quiesced-source
acknowledgment. Read a supported schema, record private hashes of every table and
sequence, validate recorded object IDs/size/SHA256, dump it and confirm the source
has not changed. Restore into a newly generated disposable database. Compare every
table and sequence and copy only database-recorded chat objects into a fresh
private application directory, verifying exact bytes. Never copy credentials,
worker/browser directories or arbitrary orphan files. Existing output directories,
remote/normal databases, unsafe schema/object paths, symlinks, missing/corrupt
objects, source drift and failed restore must fail without replacing source data.

The tool prints a compact metadata-only outcome and retains a private backup,
manifest and restore report. Cleanup can drop only the database created by this
invocation; retaining it for a controlled follow-up is explicit. Runtime grants
follow the current restricted relay_app contract. No normal database migration,
cloud action, source mutation or provider inference is part of the rehearsal.

Acceptance: meaningful guard/failure tests plus a connected synthetic workspace
with a pending ordinary question, an uncertain applied external effect, reviewed
personal Home/layout and an authenticated private file. Quiesce owned workers and
controller, rehearse the copied state, start the restored local controller and
verify those same records and file bytes under fresh original/unauthorized user
sessions. Starting an executor must not poll a model or repeat uncertain work.
Inspect the restored Home/task view and retain exact evidence.

This is a local recovery rehearsal, not an online consistent backup service,
production restore runbook, RPO/RTO promise or S3/Library recovery acceptance.

## Run on disposable data

Stop the controllers and executors that own the source test schema first. Leave
unrelated local applications alone. Set `RELAY_TEST_DATABASE_URL` through the
environment to the dedicated local simulation database; never put a credential
in command arguments or a committed file. The URL may select a local socket and
port, but must not contain `options` or service overrides.

```sh
python3 tools/workspace_restore.py \
  --schema relay_worker_example \
  --files-root /absolute/test/application/workspaces \
  --output /absolute/fresh/rehearsal \
  --source-quiesced
```

Use the actual test schema and files root. Schema 33 is the supported contract;
other versions fail. The source schema is limited to 128 MiB and recorded chat
files to 256 objects, 10 MiB each and 128 MiB total. Individual PostgreSQL commands
have a 60-second deadline and bounded captured output. The source must stay
quiesced throughout; before/after fingerprints detect ordinary drift, but do not
turn a live database/filesystem into one atomic snapshot.

The default deletes the newly created database after verification. `--keep-target`
retains a successful target for an explicit local application check; a failed
restore is cleaned up even with that flag. A cleanup failure is reported as a
failure with the owned target marked retained. Inspect `verification.json` for
its exact name. The tool never replaces or drops the source database.

`workspace.dump`, `manifest.json`, `verification.json` and the restored application
files remain in the private output directory. The manifest fingerprints every
table and sequence, and lists recorded object identities/checksums. These are
private artifacts: the database snapshot includes application records, including
session and connection metadata. It does not copy external secret files. Do not
publish a backup or assume restored sessions are invalidated automatically.

## Verified local behavior

```sh
python3 -m unittest discover -s tests -p workspace_restore_test.py
node --test tests/workspace_restore.mjs
```

The process test additionally requires `RELAY_TEST_DATABASE_URL`, the built local
binary, PostgreSQL tools, pinned prepared Goose and installed Playwright Chromium.
It creates synthetic state through actual application/Goose/MCP paths, stops only
its own processes, runs the restore, then boots the restored controller under the
restricted application role. Synthetic simulation roster and local provider
configuration are supplied separately by the test, not copied by the backup tool.

Twenty guard/failure tests and the connected rehearsal passed on 2026-10-26.
The earlier schema-30 rehearsal compared 71 tables, nine sequences and one private file. Fresh owner
and unrelated-user sessions verified Home/layout, question and file access. A new
executor and the browser showed the saved question and uncertain action with no
additional model request or repeated external write. Source records remained
unchanged and the owned target database was removed after the check. Retained
evidence: `.tmp/independent-worker-TvaXON/restore/`.

The initial connected run exposed test heartbeat-column and cleanup-order mistakes;
those were corrected before the passing rerun. Its separately owned leftover target
was explicitly cleaned up. No normal workspace or cloud resource was changed.
