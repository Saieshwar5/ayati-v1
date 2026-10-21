# Local PostgreSQL workspace foundation

Relay can serve its existing React client using PostgreSQL for accounts, sessions,
workspaces, memberships, invitations, shared conversations, human resource grants,
App-session bindings, audit records and durable shared-reply jobs. Set
`RELAY_DATABASE_URL` privately to enable it. A configured connection failure never
falls back to SQLite. The original standalone/local mode is retained.

This is a local deployment increment, not production cloud tenancy. Private agent
runtime stores, the agent registry, generated App records, panels and sourced
memory still use their existing SQLite stores. Existing workspace paths and the
explicit owner installation binding remain unchanged. Shared agents still have
conversation-only context with no private files, memory or business tools.

## Dependencies and local server

Pinned SeaORM and sea-orm-migration 2.0.4 (MIT OR Apache-2.0) use PostgreSQL through
SQLx 0.9 and the existing Tokio runtime. SeaORM entities cover stable account and
workspace records; parameterized SQL handles explicit authorization transactions,
imports and queue claims. One pool per backend has at most ten connections.
This increment was exercised with PostgreSQL 18.6.

Install PostgreSQL server and client tools with your operating system package
manager, or set `PG_BINDIR` and `PG_SHAREDIR` to a verified local extraction.
The helper also recognizes `.ayati-local/postgres-tools/usr/{bin,share/postgresql}`.
On this development host the Arch PostgreSQL package was downloaded into that
ignored directory and its detached signature verified with `pacman-key` before
extraction. No system service or public listener was installed.

```sh
./tools/local-postgres.sh start
./tools/local-postgres.sh migrate
./tools/local-postgres.sh env
```

The helper creates a private Unix socket, peer authentication and a restricted
`relay_app` role. Only the current local operating-system account is mapped to
that role. The server has no TCP listener. The printed administrator URL is for
schema changes; put the printed **RELAY_DATABASE_URL** application URL in `.env`.
Do not use the administrator role for normal application traffic.

The helper grants data access, sequence access and schema usage, but no schema
creation or database administration. It revokes audit update/delete and migration
history writes from the application role. Cloud credentials, TLS and deployment
roles require a separate configuration; these local settings are not a template
for exposing the database on a network.

## Schema changes and import

Migrations are explicitly versioned with SeaORM Migrator. API startup validates
the schema and rejects unknown versions; it does not alter tables automatically.
The first migration deliberately refuses destructive down migration. A new schema
change needs a forward migration, tested against an existing populated database.

To migrate an existing local installation:

1. Stop `ayati identity` and the standalone server; pause any active work.
2. Take a verified backup of `AYATI_DATA_DIR/identity/identity.sqlite` and keep the
   original business directories and installation binding in place.
3. Initialize an empty PostgreSQL target with the migration command and grants.
4. With the target URL set, run `cargo run --locked -- workspace-db import`.
5. Run `cargo run --locked -- workspace-db check`, then `ayati identity`.
6. Sign in again. Browser sessions and temporary App sessions are not imported.

Import acquires the existing local server/daemon locks, reads SQLite without
writing it, verifies integrity, imports in one PostgreSQL transaction, checks
record counts and preserves IDs and ordering. Invalid references, partial data or
a nonempty target abort the import. It neither merges users by email nor sends
invitations. Old queued replies become failed because they have no recoverable
job payload. Original SQLite files remain available for recovery.

The `identity/postgres-identity` fence is synced before import commits. Once present,
startup refuses SQLite fallback and requires a completed PostgreSQL import. If an
import fails or its commit result is uncertain, inspect the target import record
and restore/reconcile deliberately; do not simply delete the fence and resume both
stores. Returning to SQLite after new PostgreSQL writes requires reconciliation.
Ownership adoption/release commands reject a migrated installation until they have
a PostgreSQL-aware implementation.

## Isolation and concurrency

Database foreign keys include workspace identity for conversation participants,
messages and grants. Membership removal cascades to chat membership, resource
grants and App sessions without deleting historical messages. Workspace creation,
role changes, invitation acceptance and audit writes are transactional. Retried
workspace creation and message sends require the same identity and payload.

Mutations lock the workspace row; authorized reads use shared locks where a
multi-query read must remain consistent with membership changes. Different
workspaces can use separate pooled connections. These transactions contain no
WorkOS calls, model execution or file I/O. High-volume same-workspace workloads
may need finer locking after measurement; no cloud capacity claim is made.

Application authorization remains mandatory. PostgreSQL row-level security is
**not enabled in this increment**. Before a cloud pilot, add and test tenant RLS
with transaction-scoped identity and a non-bypassing runtime role, alongside
cross-tenant search/download/subscription tests. Agents never receive database
credentials or direct database access.

## Shared reply recovery

The message, queued reply and bounded conversation-only job payload commit
atomically. A dispatcher claims persisted jobs using `FOR UPDATE SKIP LOCKED`.
Claims have a five-minute lease and unique token; execution has the existing
three-minute timeout. Expired work can be claimed again, with at most three
attempts. Exhausted work is visibly failed. Worker publication checks the claim,
lease and current conversation/agent permissions in a transaction, so a stale
worker cannot overwrite a newer result.

Ordinary human messages never start agent work. Only explicit agent requests
produce jobs. The workspace queue now accepts up to 64 pending replies while retaining four local
execution slots. Admission beyond that returns HTTP 429. Dispatch reserves a slot
before claiming and wakes on work arrival/completion, with a 500 ms recovery poll. Retry can repeat model inference and its cost; it is safe
here only because shared replies have no external business tools. Tool-enabled
jobs need action receipts and uncertain-outcome reconciliation before automatic
recovery. Local per-request working files are still used; distributed workers and
cross-host recovery are not implemented.

## Tests and recovery rehearsal

```sh
./tools/local-postgres.sh test
cargo test --locked --lib
cargo clippy --locked --lib --tests -- -D warnings
./scripts/check-file-lines.sh
```

PostgreSQL tests are explicitly ignored in the normal suite and must be invoked
by the helper (or with `RELAY_TEST_DATABASE_URL` and `--ignored`). They require a
**disposable** local database and the peer-mapped `relay_app` role. Each test creates
its own schema, then exercises application operations with the restricted role.
Schemas remain in the test database for inspection; never point these tests at
production. Tests cover identity, parallel idempotency, role boundaries, invitations,
chat isolation, revocation, concurrent messages, worker claims, import preservation,
rollback and database-role restrictions. They use synthetic replies and no paid
model calls or real invitations.

Use `pg_dump -Fc` and `pg_restore --exit-on-error` into a separate empty local
database for a logical backup rehearsal. Compare records and sequence values,
then restart PostgreSQL and repeat the reads. Backups contain private information;
keep them under an ignored directory with restrictive permissions. This proves a
local logical restore, not point-in-time recovery, managed failover or an SLA.

Remaining cloud gates: RLS, durable login-flow/session-revocation strategy,
HTTPS/Secure cookies, distributed runtime ownership, object storage, retention,
rate limits, managed backup/PITR, migration orchestration and workload benchmarks.

## Verified locally on 2026-10-20

- Nine real PostgreSQL tests passed, running normal operations as `relay_app`.
- All 45 library tests passed; the nine database tests are separately invoked.
- Ten client test files, the TypeScript/Vite build, Rust clippy and the 277-file
  source line-limit check passed. The full browser automation suite was not run.
- Both a synthetic test database and the actual imported workspace were restored
  into separate local databases. Exported rows and sequence values matched.
- PostgreSQL was restarted successfully. The existing two users, workspace,
  memberships, invitation and shared history were imported with count checks.
- Real hosted sign-in opened the original Home. Users and resource-access details
  loaded. The existing integration conversation retained its history, and one
  real Goose reply completed through the PostgreSQL queue. No invitation was sent
  and no employee permission was changed during visual verification.

Configured credentials, server binaries, database files and backups are ignored
local files; none belong in a commit. These checks establish the local boundary
above, not production capacity, distributed execution or managed recovery.

## Company collaboration and telemetry migrations — 2026-10-21

The fourth forward migration adds shared agent profiles, teams, channel visibility
and posting controls, file metadata, feedback, internal work records and receipts.
The fifth adds queue timing columns and a pending-message index. Existing job timing
is left unknown rather than invented. Startup now requires all five migrations.
New company features require PostgreSQL; SQLite keeps its previous local scope.
The dedicated simulation helper does not migrate the ordinary workspace database.
See [company simulation](company-simulation.md) for API behavior, local object-store
limits, validation results, replay and backup/restore procedures.
