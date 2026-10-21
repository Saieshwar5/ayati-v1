"""Restore an isolated simulation backup and compare every public table and sequence."""
import hashlib
import json
import pathlib
import subprocess
import time
root = pathlib.Path(__file__).resolve().parents[2] / '.ayati-local/company-simulation'
socket = str(root / 'socket')
backup = sorted((root / 'backups').iterdir())[-1]
target = 'relay_simulation_restore_' + str(time.time_ns())
base = ['-h', socket, '-p', '55433']
subprocess.run(['createdb', *base, target], check=True)
subprocess.run(['pg_restore', *base, '--exit-on-error', '-d', target, str(backup / 'company.dump')], check=True)
def sql(database, statement):
    return subprocess.check_output(['psql', '-X', *base, '-d', database, '-At', '-v', 'ON_ERROR_STOP=1', '-c', statement])
tables = sql('relay_simulation_company', "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename").decode().splitlines()
results = []
for table in tables:
    identifier = '"' + table.replace('"', '""') + '"'
    statement = f'SELECT row_to_json(t)::text FROM {identifier} t ORDER BY row_to_json(t)::text'
    source, restored = (sql(db, statement) for db in ('relay_simulation_company', target))
    assert source == restored, table
    results.append({'table': table, 'rows': len(source.splitlines()), 'sha256': hashlib.sha256(source).hexdigest()})
statement = "SELECT sequencename,last_value FROM pg_sequences WHERE schemaname='public' ORDER BY sequencename"
assert sql('relay_simulation_company', statement) == sql(target, statement)
result = {'passed': True, 'restored_database': target, 'backup': str(backup), 'tables': results, 'sequences_match': True}
(backup / 'restore-result.json').write_text(json.dumps(result, indent=2))
print(json.dumps({'passed': True, 'tables': len(tables), 'restored_database': target, 'backup': str(backup)}, indent=2))
