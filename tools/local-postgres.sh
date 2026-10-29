#!/usr/bin/env bash
# Private local PostgreSQL for Relay. No TCP listener and no database passwords.
set -euo pipefail
relay_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
relay_local="$relay_root/.ayati-local"
relay_data="$relay_local/postgres-data"
relay_socket="$relay_local/postgres-socket"
relay_port=55432
relay_owner=$(id -un)
[[ "$relay_owner" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo 'Unsupported local account name'; exit 1; }
if [[ -n "${PG_BINDIR:-}" ]]; then
    relay_bin="$PG_BINDIR"
elif command -v initdb >/dev/null; then
    relay_bin=$(dirname "$(command -v initdb)")
else
    relay_bin="$relay_local/postgres-tools/usr/bin"
fi
relay_share="${PG_SHAREDIR:-$relay_local/postgres-tools/usr/share/postgresql}"
relay_lib="$relay_local/postgres-tools/usr/lib/postgresql"
relay_admin="postgresql://$relay_owner@localhost/relay_workspace?host=$relay_socket&port=$relay_port"
relay_test="postgresql://$relay_owner@localhost/relay_workspace_test?host=$relay_socket&port=$relay_port"
relay_runtime="postgresql://relay_app@localhost/relay_workspace?host=$relay_socket&port=$relay_port"
psql_local() { psql -X -h "$relay_socket" -p "$relay_port" -U "$relay_owner" -v ON_ERROR_STOP=1 "$@"; }
case "${1:-help}" in
    start)
        [[ -x "$relay_bin/initdb" ]] || { echo 'Install PostgreSQL server tools or set PG_BINDIR and PG_SHAREDIR. See runtime/postgres-workspaces.md.'; exit 1; }
        mkdir -p "$relay_socket"
        chmod 700 "$relay_socket"
        if [[ ! -f "$relay_data/PG_VERSION" ]]; then
            relay_init=()
            [[ ! -d "$relay_share" ]] || relay_init=(-L "$relay_share")
            "$relay_bin/initdb" -D "$relay_data" "${relay_init[@]}" --auth-local=peer --auth-host=scram-sha-256 --encoding=UTF8 --locale=C.UTF-8 --no-instructions
        fi
        if ! rg -q '^local all relay_app peer map=relay_app_local$' "$relay_data/pg_hba.conf"; then
            { printf 'local all relay_app peer map=relay_app_local\n'; cat "$relay_data/pg_hba.conf"; } > "$relay_data/pg_hba.conf.new"
            mv "$relay_data/pg_hba.conf.new" "$relay_data/pg_hba.conf"
        fi
        if ! rg -q "^relay_app_local $relay_owner relay_app$" "$relay_data/pg_ident.conf"; then
            printf '\nrelay_app_local %s relay_app\n' "$relay_owner" >> "$relay_data/pg_ident.conf"
        fi
        if ! "$relay_bin/pg_ctl" -D "$relay_data" status >/dev/null 2>&1; then
            relay_options="-k '$relay_socket' -p $relay_port -c listen_addresses=''"
            [[ ! -d "$relay_lib" ]] || relay_options+=" -c dynamic_library_path='$relay_lib'"
            "$relay_bin/pg_ctl" -D "$relay_data" -l "$relay_data/server.log" -o "$relay_options" start
        fi
        psql_local -d postgres -c 'SELECT pg_reload_conf();' >/dev/null
        psql_local -d postgres <<'SQL'
DO $$ BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname='relay_app') THEN
        CREATE ROLE relay_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;
    END IF;
END $$;
SQL
        for relay_db in relay_workspace relay_workspace_test; do
            if [[ $(psql_local -d postgres -Atc "SELECT count(*) FROM pg_database WHERE datname='$relay_db'") == 0 ]]; then
                createdb -h "$relay_socket" -p "$relay_port" -U "$relay_owner" "$relay_db"
            fi
        done
        echo 'Local PostgreSQL ready on a private Unix socket.'
        ;;
    migrate)
        cd "$relay_root"
        RELAY_DATABASE_URL="$relay_admin" cargo run --locked -- workspace-db migrate
        psql_local -d relay_workspace <<'SQL'
GRANT CONNECT ON DATABASE relay_workspace TO relay_app;
GRANT USAGE ON SCHEMA public TO relay_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO relay_app;
GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO relay_app;
REVOKE UPDATE,DELETE ON relay_audit FROM relay_app;
REVOKE INSERT,UPDATE,DELETE ON seaql_migrations FROM relay_app;
SQL
        ;;
    test)
        cd "$relay_root"
        # Every ignored library test needs this database, except the S3 emulator check.
        RELAY_TEST_DATABASE_URL="$relay_test" cargo test --locked --lib identity:: -- --ignored \
            --skip library::storage::tests::s3_compatible_objects_support_conditional_creation_and_integrity
        ;;
    env)
        printf 'RELAY_DATABASE_URL=%s\nRELAY_ADMIN_DATABASE_URL=%s\nRELAY_TEST_DATABASE_URL=%s\n' "$relay_runtime" "$relay_admin" "$relay_test"
        ;;
    stop) "$relay_bin/pg_ctl" -D "$relay_data" -m fast stop ;;
    *) echo 'Usage: tools/local-postgres.sh start|migrate|test|env|stop' ;;
esac
