#!/usr/bin/env bash
# Dedicated local cluster and persistent company data. Never cleans up records.
set -euo pipefail
company_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
company_local="$company_root/.ayati-local/company-simulation"
company_socket="$company_local/socket"
company_data="$company_local/postgres"
company_bin="${PG_BINDIR:-$company_root/.ayati-local/postgres-tools/usr/bin}"
company_share="${PG_SHAREDIR:-$company_root/.ayati-local/postgres-tools/usr/share/postgresql}"
company_lib="$company_root/.ayati-local/postgres-tools/usr/lib/postgresql"
company_owner=$(id -un)
company_port=55433
company_url="postgresql://$company_owner@localhost/relay_simulation_company?host=$company_socket&port=$company_port"
company_runtime="postgresql://relay_app@localhost/relay_simulation_company?host=$company_socket&port=$company_port"
company_psql() { psql -X -h "$company_socket" -p "$company_port" -U "$company_owner" -v ON_ERROR_STOP=1 "$@"; }
cd "$company_root"
case "${1:-help}" in
  setup)
    mkdir -p "$company_socket" "$company_local/data"
    chmod 700 "$company_local" "$company_socket" "$company_local/data"
    if [[ ! -f "$company_data/PG_VERSION" ]]; then
      "$company_bin/initdb" -D "$company_data" -L "$company_share" --auth-local=peer --auth-host=reject --encoding=UTF8 --locale=C.UTF-8 --no-instructions
    fi
    if ! rg -q '^local all relay_app peer map=company_app$' "$company_data/pg_hba.conf"; then
      { printf 'local all relay_app peer map=company_app\n'; cat "$company_data/pg_hba.conf"; } > "$company_data/pg_hba.conf.new"
      mv "$company_data/pg_hba.conf.new" "$company_data/pg_hba.conf"
      printf '\ncompany_app %s relay_app\n' "$company_owner" >> "$company_data/pg_ident.conf"
    fi
    if ! "$company_bin/pg_ctl" -D "$company_data" status >/dev/null 2>&1; then
      "$company_bin/pg_ctl" -D "$company_data" -l "$company_local/postgres.log" -o "-k '$company_socket' -p $company_port -c listen_addresses='' -c dynamic_library_path='$company_lib'" start
    fi
    company_psql -d postgres -c 'SELECT pg_reload_conf();' >/dev/null
    company_psql -d postgres -c "DO \$\$ BEGIN IF NOT EXISTS(SELECT FROM pg_roles WHERE rolname='relay_app') THEN CREATE ROLE relay_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE; END IF; END \$\$;"
    for company_db in relay_simulation_company relay_simulation_test; do
      if [[ $(company_psql -d postgres -Atc "SELECT count(*) FROM pg_database WHERE datname='$company_db'") == 0 ]]; then
        createdb -h "$company_socket" -p "$company_port" -U "$company_owner" "$company_db"
      fi
    done
    RELAY_DATABASE_URL="$company_url" AYATI_DATA_DIR="$company_local/data" target/debug/ayati workspace-db migrate
    company_psql -d relay_simulation_company <<'SQL'
GRANT CONNECT ON DATABASE relay_simulation_company TO relay_app;
GRANT USAGE ON SCHEMA public TO relay_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO relay_app;
GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO relay_app;
REVOKE UPDATE,DELETE ON relay_audit FROM relay_app;
REVOKE INSERT,UPDATE,DELETE ON seaql_migrations FROM relay_app;
SQL
    ;;
  serve)
    export RELAY_SIMULATION_DATABASE_URL="$company_runtime" AYATI_PORT="${COMPANY_HTTP_PORT:-8876}"
    exec target/debug/ayati simulate "$company_local/data"
    ;;
  run) exec node tools/company-simulation/run.mjs "http://127.0.0.1:${COMPANY_HTTP_PORT:-8876}" "$company_local/runs" ;;
  test)
    export RELAY_TEST_DATABASE_URL="postgresql://$company_owner@localhost/relay_simulation_test?host=$company_socket&port=$company_port"
    exec cargo test --locked --lib identity::postgres -- --ignored
    ;;
  backup)
    company_backup="$company_local/backups/$(date -u +%Y%m%dT%H%M%S)-$$"
    mkdir -p "$company_backup"
    pg_dump -h "$company_socket" -p "$company_port" -U "$company_owner" -Fc relay_simulation_company -f "$company_backup/company.dump"
    cp -a "$company_local/data" "$company_backup/data"
    cp -a "$company_local/runs" "$company_backup/runs"
    echo "Saved $company_backup (stop the simulation runner before taking coordinated backups)."
    ;;
  *) echo 'Usage: tools/company-simulation.sh setup|serve|run|test|backup' ;;
esac
