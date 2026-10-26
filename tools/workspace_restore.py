#!/usr/bin/env python3
"""Rehearse a quiesced local simulation restore; never replace its source."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import stat
import subprocess
import time
from urllib.parse import parse_qs, unquote, urlsplit
import uuid

MAX_BYTES = 128 * 1024 * 1024
SCHEMA_VERSION = 32
COMMAND_SECONDS = 60


class RestoreError(Exception):
    """An intentionally credential-free operator failure."""


def require(value, message):
    if not value:
        raise RestoreError(message)


def connection(value):
    try:
        url = urlsplit(value)
        query = parse_qs(url.query, strict_parsing=True, keep_blank_values=True)
        database = unquote(url.path.lstrip('/'))
        port = query.get('port', [str(url.port or 5432)])[0]
        host = query.get('host', [url.hostname])[0]
        require(url.scheme in ('postgres', 'postgresql') and not url.fragment,
                'Use a PostgreSQL connection without a fragment')
        require(url.hostname in ('localhost', '127.0.0.1'), 'Only loopback PostgreSQL is supported')
        require(re.fullmatch(r'relay_simulation_[a-z0-9_]{1,42}', database),
                'Only disposable relay_simulation_* databases are supported')
        require(set(query) <= {'host', 'port'} and all(len(v) == 1 for v in query.values()),
                'Connection options and service overrides are not supported')
        require(host in ('localhost', '127.0.0.1') or (host.startswith('/') and Path(host).is_dir()),
                'Use loopback or an existing local PostgreSQL socket directory')
        require(port.isdigit() and 1 <= int(port) <= 65535, 'Invalid PostgreSQL port')
        env = {k: v for k, v in os.environ.items() if not k.startswith('PG')}
        env.update(PGDATABASE=database, PGHOST=host, PGPORT=port,
                   PGUSER=unquote(url.username or os.environ.get('USER', '')),
                   PGPASSWORD=unquote(url.password or ''), PGCLIENTENCODING='UTF8',
                   PGPASSFILE='/dev/null', PGCONNECT_TIMEOUT='5')
        return env
    except RestoreError:
        raise
    except (ValueError, TypeError, AttributeError):
        raise RestoreError('Invalid local simulation connection') from None


def identifier(value):
    return '"' + value.replace('"', '""') + '"'


def schema_name(value):
    require(bool(re.fullmatch(r'(?:public|relay_[a-z0-9_]{1,56})', value)),
            'Use an explicit simulation schema identifier')
    return value


def command(args, env, data=None):
    """Bound output while reading it; never retain provider/database stderr."""
    process = None
    deadline = time.monotonic() + COMMAND_SECONDS
    try:
        process = subprocess.Popen(args, stdin=subprocess.PIPE if data else subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
        output = bytearray()
        sent = 0
        with selectors.DefaultSelector() as streams:
            for stream, kind in [(process.stdout, 'out'), (process.stderr, 'err')]:
                os.set_blocking(stream.fileno(), False)
                streams.register(stream, selectors.EVENT_READ, kind)
            if data:
                os.set_blocking(process.stdin.fileno(), False)
                streams.register(process.stdin, selectors.EVENT_WRITE, 'in')
            while streams.get_map():
                remaining = deadline - time.monotonic()
                require(remaining > 0, 'Local PostgreSQL command timed out: ' + args[0])
                for key, _ in streams.select(min(remaining, 0.25)):
                    if key.data == 'in':
                        sent += os.write(key.fd, data[sent:sent + 16384])
                        if sent == len(data):
                            streams.unregister(key.fileobj)
                            key.fileobj.close()
                    else:
                        chunk = os.read(key.fd, 65536)
                        if not chunk:
                            streams.unregister(key.fileobj)
                            key.fileobj.close()
                        elif key.data == 'out':
                            require(len(output) + len(chunk) <= MAX_BYTES,
                                    'PostgreSQL output exceeds the rehearsal bound')
                            output.extend(chunk)
            result = process.wait(timeout=max(0.01, deadline - time.monotonic()))
        require(result == 0, 'Local PostgreSQL command failed: ' + args[0])
        return bytes(output)
    except (OSError, ValueError, subprocess.TimeoutExpired):
        raise RestoreError('Local PostgreSQL command unavailable or timed out: ' + args[0]) from None
    finally:
        if process is not None:
            if process.poll() is None:
                process.kill()
            process.wait()
            for stream in [process.stdin, process.stdout, process.stderr]:
                if stream is not None:
                    stream.close()


def sql(env, statement):
    return command(['psql', '-X', '-qAt', '-v', 'ON_ERROR_STOP=1'], env,
                   ("SET timezone='UTC'; SET datestyle='ISO,YMD';\n" + statement).encode())


def snapshot(env, schema):
    name = schema_name(schema)
    tables = sql(env, f"SELECT tablename FROM pg_tables WHERE schemaname='{name}' ORDER BY tablename COLLATE \"C\";").decode().splitlines()
    require('seaql_migrations' in tables, 'Workspace migration records are missing')
    versions = sql(env, f'SELECT version FROM {identifier(name)}.seaql_migrations ORDER BY version;').decode().splitlines()
    require(len(versions) == SCHEMA_VERSION and versions[-1] == 'm20261026_000032_native_communications',
            'Use the current supported workspace schema 32')
    total = int(sql(env, f"SELECT coalesce(sum(pg_total_relation_size(c.oid)),0) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='{name}' AND c.relkind IN ('r','m');"))
    require(total <= MAX_BYTES, 'Simulation schema exceeds the 128 MiB rehearsal bound')
    result = []
    for table in tables:
        relation = identifier(name) + '.' + identifier(table)
        rows = sql(env, f'SELECT row_to_json(t)::text FROM {relation} t ORDER BY row_to_json(t)::text COLLATE "C";')
        result.append({'table': table, 'rows': len(rows.splitlines()), 'sha256': hashlib.sha256(rows).hexdigest()})
    sequences = []
    names = sql(env, f"SELECT sequencename FROM pg_sequences WHERE schemaname='{name}' ORDER BY sequencename COLLATE \"C\";").decode().splitlines()
    for sequence in names:
        value = sql(env, f'SELECT last_value,is_called FROM {identifier(name)}.{identifier(sequence)};').decode().strip()
        sequences.append({'sequence': sequence, 'state': value})
    return {'tables': result, 'sequences': sequences, 'schema_version': len(versions)}


def objects(env, schema):
    result = sql(env, f"SELECT coalesce(json_agg(o ORDER BY workspace,id),'[]') FROM (SELECT workspace,id,size,checksum FROM {identifier(schema_name(schema))}.relay_objects) o;")
    records = json.loads(result)
    require(isinstance(records, list) and len(records) <= 256,
            'Chat object collection exceeds the rehearsal bound')
    for item in records:
        object_identity(item)
        require(type(item.get('size')) is int and 0 < item['size'] <= 10 * 1024 * 1024,
                'Invalid recorded chat object size')
        require(isinstance(item.get('checksum'), str)
                and bool(re.fullmatch('[a-f0-9]{64}', item['checksum'])), 'Invalid recorded object checksum')
    require(sum(r['size'] for r in records) <= MAX_BYTES, 'Chat object collection exceeds the rehearsal bound')
    return records


def object_identity(record):
    require(isinstance(record, dict), 'Invalid recorded chat object')
    for key in ('workspace', 'id'):
        try:
            require(str(uuid.UUID(record[key])) == record[key], 'Object identity must be a canonical UUID')
        except (KeyError, ValueError, TypeError, AttributeError):
            raise RestoreError('Unsafe recorded object identity') from None


def object_path(root, record):
    # IDs have already been validated, but reject any symlink in the actual path too.
    object_identity(record)
    parts = [root, root / record['workspace'], root / record['workspace'] / 'objects']
    path = parts[-1] / record['id']
    require(all(not p.is_symlink() for p in [*parts, path]), 'Symbolic links are not supported in chat objects')
    return path


@contextmanager
def open_object(root, record):
    """Walk directory descriptors without following links, including ancestors."""
    object_identity(record)
    directory = None
    try:
        flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
        directory = os.open('/', flags)
        for part in [*Path(root).absolute().parts[1:], record['workspace'], 'objects']:
            next_directory = os.open(part, flags, dir_fd=directory)
            os.close(directory)
            directory = next_directory
        descriptor = os.open(record['id'], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                             dir_fd=directory)
        with os.fdopen(descriptor, 'rb') as handle:
            require(stat.S_ISREG(os.fstat(handle.fileno()).st_mode),
                    'Recorded chat object must be a regular file')
            yield handle
    except OSError:
        raise RestoreError('A recorded chat object is missing, linked or unreadable') from None
    finally:
        if directory is not None:
            os.close(directory)


def verify_objects(root, records):
    root = Path(root).absolute()
    require(root.is_dir() and root == root.resolve(), 'Use a real local chat-object root')
    for item in records:
        with open_object(root, item) as handle:
            info = os.fstat(handle.fileno())
            require(info.st_size == item['size'], 'Recorded chat object size/type mismatch')
            digest = object_bytes(handle, item['size'])
        require(digest == item['checksum'], 'Recorded chat object checksum mismatch')


def object_bytes(handle, expected, destination=None):
    digest = hashlib.sha256()
    total = 0
    while chunk := handle.read(min(65536, expected - total + 1)):
        total += len(chunk)
        require(total <= expected, 'Recorded chat object grew during verification')
        digest.update(chunk)
        if destination is not None:
            destination.write(chunk)
    require(total == expected, 'Recorded chat object size changed during verification')
    return digest.hexdigest()


def copy_objects(source, target, records):
    verify_objects(source, records)
    target.mkdir(parents=True, mode=0o700, exist_ok=False)
    for item in records:
        destination = object_path(target, item)
        destination.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
        with open_object(source, item) as src, destination.open('xb') as dst:
            digest = object_bytes(src, item['size'], dst)
            require(digest == item['checksum'], 'Recorded chat object changed during copy')
            dst.flush()
            os.fsync(dst.fileno())
        destination.chmod(0o600)
    verify_objects(target, records)


def private_json(path, value):
    with path.open('x', encoding='utf-8') as handle:
        json.dump(value, handle, indent=2)
        handle.write('\n')
    path.chmod(0o600)


def rehearse(env, schema, files_root, output, keep_target=False):
    schema_name(schema)
    output = Path(output).absolute()
    require(output == output.resolve(), 'Use a real output path without symbolic links')
    require(not output.is_relative_to(Path(files_root).resolve()),
            'Output must be outside the source chat-object tree')
    require(not output.exists(), 'Output directory already exists; choose a fresh rehearsal path')
    try:
        output.mkdir(parents=True, mode=0o700, exist_ok=False)
    except OSError:
        raise RestoreError('Cannot create the private rehearsal directory') from None
    old_mask = os.umask(0o077)
    target = 'relay_simulation_restore_' + uuid.uuid4().hex
    created = False
    report = {'passed': False, 'schema': schema, 'source_database': env['PGDATABASE'],
              'target_database': target, 'target_retained': False}
    try:
        before = snapshot(env, schema)
        recorded = objects(env, schema)
        verify_objects(files_root, recorded)
        archive = output / 'workspace.dump'
        command(['pg_dump', '--format=custom', '--no-owner', '--no-acl', '--schema=' + schema,
                 '--file=' + str(archive)], env)
        archive.chmod(0o600)
        require(archive.stat().st_size <= MAX_BYTES, 'Backup archive exceeds the rehearsal bound')
        require(snapshot(env, schema) == before and objects(env, schema) == recorded,
                'Source changed during backup; quiesce it and retry in a fresh directory')
        command(['createdb', '--maintenance-db=' + env['PGDATABASE'], '--template=template0', target], env)
        created = True
        restored = {**env, 'PGDATABASE': target}
        command(['pg_restore', '--exit-on-error', '--no-owner', '--no-acl', '--dbname=' + target, str(archive)], restored)
        require(snapshot(restored, schema) == before, 'Restored table or sequence fingerprint differs')
        target_files = output / 'application' / 'workspaces'
        copy_objects(Path(files_root), target_files, recorded)
        require(snapshot(env, schema) == before, 'Source drifted before verification finished')
        verify_objects(files_root, recorded)
        name = identifier(schema)
        sql(restored, f'GRANT USAGE ON SCHEMA {name} TO relay_app; GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA {name} TO relay_app; GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA {name} TO relay_app; REVOKE UPDATE,DELETE ON {name}.relay_audit FROM relay_app; REVOKE INSERT,UPDATE,DELETE ON {name}.seaql_migrations FROM relay_app;')
        report.update(passed=True, tables=len(before['tables']), sequences=len(before['sequences']),
                      objects=len(recorded), source_unchanged=True, schema_version=SCHEMA_VERSION,
                      target_files=str(target_files), target_retained=keep_target)
        private_json(output / 'manifest.json', {**before, 'objects': recorded})
    except RestoreError as error:
        report['passed'] = False
        report['error'] = str(error)
        raise
    except (OSError, ValueError, TypeError, KeyError):
        report['passed'] = False
        report['error'] = 'Invalid or unavailable local restore data'
        raise RestoreError(report['error']) from None
    finally:
        if created and (not keep_target or not report['passed']):
            try:
                command(['dropdb', '--maintenance-db=' + env['PGDATABASE'], target], env)
                report['target_retained'] = False
            except RestoreError:
                report['cleanup_error'] = 'Owned disposable database cleanup failed'
                report['target_retained'] = True
                report['passed'] = False
        try:
            private_json(output / 'verification.json', report)
        except OSError:
            raise RestoreError('Cannot retain the private verification report') from None
        finally:
            os.umask(old_mask)
    require(report['passed'], 'Rehearsal or owned database cleanup failed')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--schema', required=True)
    parser.add_argument('--files-root', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--source-quiesced', action='store_true', help='Confirm owned controllers/workers are stopped')
    parser.add_argument('--keep-target', action='store_true', help='Keep only this newly generated database for follow-up checks')
    args = parser.parse_args()
    try:
        require(args.source_quiesced, 'Stop source controllers/workers and pass --source-quiesced')
        env = connection(os.environ.get('RELAY_TEST_DATABASE_URL', ''))
        result = rehearse(env, args.schema, args.files_root, args.output, args.keep_target)
        print(json.dumps({k: result[k] for k in ('passed', 'tables', 'sequences', 'objects', 'target_database', 'target_retained')}))
        return 0
    except RestoreError as error:
        print(json.dumps({'passed': False, 'error': str(error)}))
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
