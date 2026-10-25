#!/usr/bin/env python3
"""Fresh private PostgreSQL and verified transient resource limits for agent tests."""
import argparse
from datetime import datetime, timezone
import getpass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
import time
from urllib.parse import quote, urlencode
import uuid


ROOT = Path(__file__).resolve().parent.parent
SUITES = {
    'matrix': ['tests/agent_load.mjs'],
    'faults': ['tests/agent_load_faults.mjs'],
    'preparation': ['tests/harness_preparation.mjs'],
    'mixed': ['tests/interactive_reserve.mjs'],
    'image': ['tests/image_release.mjs', 'tests/interactive_reserve.mjs', 'tests/agent_load_faults.mjs'],
    'calibration': ['tests/provider_calibration.mjs'],
    'regressions': ['tests/reply_stream.mjs', 'tests/reply_capacity.mjs',
                    'tests/executor_pool.mjs', 'tests/worker_control.mjs',
                    'tests/assignment_workers.mjs', 'tests/independent_chat_worker.mjs',
                    'tests/hosted_deployment.mjs'],
}


def run_checked(command, env, log, input_text=None):
    with log.open('a') as output:
        result = subprocess.run(command, cwd=ROOT, env=env, input=input_text,
                                text=True, stdout=output, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError(f'Local setup failed; inspect {log.name}')


def scope(profile, role, command):
    limit = profile['roles'][role]
    name = f"{profile['prefix']}-{role}-{uuid.uuid4().hex}.scope"
    with Path(profile['units_file']).open('a') as file:
        file.write(name + '\n')
    return ['systemd-run', '--user', '--scope', '--quiet', '--unit=' + name,
            '-p', f"CPUQuota={limit['cpu_percent']}%",
            '-p', f"MemoryMax={limit['memory_bytes']}", '-p', 'MemorySwapMax=0',
            '-p', f"TasksMax={limit['tasks']}",
            'taskset', '--cpu-list', profile['cpu_list'], *map(str, command)]


def snapshot(pid):
    relative = Path(f'/proc/{pid}/cgroup').read_text().split('0::', 1)[1].strip()
    directory = Path('/sys/fs/cgroup') / relative.lstrip('/')
    names = ['cpu.max', 'memory.max', 'memory.swap.max', 'pids.max',
             'cpu.stat', 'memory.current', 'memory.peak', 'memory.events']
    return {'path': relative, **{name: (directory / name).read_text().strip() for name in names}}


def verify_limits(actual, profile, role):
    wanted = profile['roles'][role]
    quota, period = map(int, actual['cpu.max'].split())
    events = dict(line.split() for line in actual['memory.events'].splitlines())
    checks = [(quota / period == wanted['cpu_percent'] / 100, 'CPU quota missing'),
              (actual['memory.max'] == str(wanted['memory_bytes']), 'Memory limit missing'),
              (actual['memory.swap.max'] == '0', 'Swap limit missing'),
              (actual['pids.max'] == str(wanted['tasks']), 'Task limit missing'),
              (actual['path'].split('/')[-1].startswith(profile['prefix'] + '-' + role + '-'), 'Foreign resource scope'),
              (int(events['oom_kill']) == 0, 'Resource profile killed a PostgreSQL process')]
    for allowed, message in checks:
        if not allowed:
            raise RuntimeError(message)


def stop_scopes(profile):
    # Only this invocation's recorded UUID-prefixed units can be stopped.
    for unit in reversed(Path(profile['units_file']).read_text().splitlines()):
        if not re.fullmatch(re.escape(profile['prefix']) + r'-[a-z]+-[a-f0-9]{32}\.scope', unit):
            raise RuntimeError('Unexpected owned unit name')
        subprocess.run(['systemctl', '--user', 'stop', unit], capture_output=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', choices=SUITES, default='matrix')
    parser.add_argument('--image', help='Immutable sha256 image ID; container mode for image/matrix/mixed/faults/calibration suites')
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/ayati')
    parser.add_argument('--postgres-bin', type=Path)
    parser.add_argument('--samples', type=int, default=24)
    parser.add_argument('--repeats', type=int, default=2)
    parser.add_argument('--concurrency', default='1,4,8,16')
    parser.add_argument('--contexts', default='small,large')
    parser.add_argument('--executor-cpu', type=int, choices=[100, 200], default=100,
                        help='CPU ceiling per two-slot executor, in percent of one CPU')
    args = parser.parse_args()
    if args.image and args.suite not in ['image', 'matrix', 'mixed', 'faults', 'calibration']:
        parser.error('This suite does not support container mode')
    if args.suite == 'image' and not args.image:
        parser.error('The image suite requires --image sha256:...')
    if args.suite == 'calibration' and (not args.image or not os.environ.get('AYATI_CALIBRATION_CONFIG')):
        parser.error('Use tools/calibrate-provider.mjs for isolated image calibration')
    if not args.image and not args.binary.is_file():
        parser.error('Build ayati first or supply --binary')
    if not 4 <= args.samples <= 40 or not 1 <= args.repeats <= 5:
        parser.error('Samples must be 4-40 and repeats 1-5')
    if not set(args.concurrency.split(',')) <= {'1', '4', '8', '16'}:
        parser.error('Unsupported concurrency')
    if not set(args.contexts.split(',')) <= {'small', 'large'}:
        parser.error('Unsupported contexts')
    pg_bin = args.postgres_bin or Path(os.environ.get('PG_BINDIR',
        str(Path(shutil.which('initdb')).parent) if shutil.which('initdb')
        else str(ROOT / '.ayati-local/postgres-tools/usr/bin')))
    if not all((pg_bin / name).is_file() for name in ['initdb', 'postgres']):
        parser.error('PostgreSQL server tools missing; supply --postgres-bin')
    if not all(shutil.which(name) for name in ['systemd-run', 'systemctl', 'psql', 'taskset', 'node']):
        parser.error('Requires systemd user manager, psql, taskset and Node')
    cpus = sorted(os.sched_getaffinity(0))[:4]
    if len(cpus) != 4:
        parser.error('The named profile requires four available logical CPUs')

    (ROOT / '.tmp').mkdir(exist_ok=True)
    evidence = Path(tempfile.mkdtemp(prefix='bounded-agent-', dir=ROOT / '.tmp'))
    os.chmod(evidence, 0o700)
    units = evidence / 'owned-units.txt'
    units.touch(mode=0o600)
    roles = {role: {'cpu_percent': 100, 'memory_bytes': mib * 1024 * 1024, 'tasks': 256}
             for role, mib in [('driver', 512), ('controller', 512), ('executor', 1024), ('postgres', 512)]}
    roles['executor']['cpu_percent'] = args.executor_cpu
    profile = {'version': 1, 'prefix': 'ayati-load-' + uuid.uuid4().hex[:16],
               'cpu_list': ','.join(map(str, cpus)), 'roles': roles, 'units_file': str(units),
               'limits': 'Per-role ceilings, not exclusive reservations; shared CPU and disk remain external contention sources.'}
    profile_file = evidence / 'profile.json'
    profile_file.write_text(json.dumps(profile, indent=2) + '\n')
    binary = evidence / 'ayati'
    if not args.image:
        shutil.copy2(args.binary.resolve(), binary)
    admin = getpass.getuser()
    if not re.fullmatch(r'[a-zA-Z0-9_-]+', admin):
        raise RuntimeError('Unsupported local account name')
    cluster, socket = evidence / 'postgres', evidence / 'socket'
    socket.mkdir(mode=0o700)
    dbname = 'relay_simulation_bounded_' + uuid.uuid4().hex
    # A developer's libpq service/hostaddr/SSL options must not redirect a private
    # test connection or change its behavior. Only this owned cluster is allowed.
    env = {key: value for key, value in os.environ.items() if not key.startswith('PG')}
    env.update(PGHOST=str(socket), PGPORT='55439', PGUSER=admin,
               PGDATABASE='postgres', PGPASSWORD='', PGOPTIONS='')
    metadata = {'version': 1, 'began_utc': datetime.now(timezone.utc).isoformat(),
                'suite': args.suite, 'profile': profile, 'database': dbname,
                'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest() if not args.image else None,
                'base_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                'exit_code': None, 'errors': []}
    goose = ROOT / '.cache/goose/v1.54.0/goose'
    if goose.is_file() and not args.image:
        metadata['goose_binary_sha256'] = hashlib.sha256(goose.read_bytes()).hexdigest()
    prepared = ROOT / '.cache/goose/v1.54.0-ayati.1/goose'
    if prepared.is_file() and not args.image:
        with prepared.open('rb') as file:
            metadata['prepared_goose_sha256'] = hashlib.file_digest(file, 'sha256').hexdigest()
    source_paths = subprocess.check_output(['git', 'ls-files', '--cached', '--others',
        '--exclude-standard', '--', 'src', 'tests', 'tools'], cwd=ROOT, text=True).splitlines()
    metadata['checkout_source_sha256'] = {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
        for name in source_paths if (ROOT / name).is_file() and (ROOT / name).suffix in {'.rs', '.mjs', '.py', '.sql'}}
    save = lambda: (evidence / 'result.json').write_text(json.dumps(metadata, indent=2) + '\n')
    image_config = None
    if args.image:
        import agent_image_resources
        image_config, image_file, metadata['image'] = agent_image_resources.prepare(evidence, args.image)
        env['PGPORT'] = str(image_config['port'])
        env['AYATI_TEST_IMAGE_CONFIG'] = str(image_file)
    pg = None
    output = None
    save()
    print('Retained bounded rehearsal:', evidence, flush=True)
    try:
        init = [str(pg_bin / 'initdb'), '-D', str(cluster), '--auth-local=peer',
                '--auth-host=reject', '--encoding=UTF8', '--locale=C.UTF-8', '--no-instructions']
        share = Path(os.environ.get('PG_SHAREDIR', str(pg_bin.parent / 'share/postgresql')))
        if share.is_dir():
            init += ['-L', str(share)]
        run_checked(init, env, evidence / 'setup.log')
        hba = cluster / 'pg_hba.conf'
        tcp_hba = 'host all relay_app,relay_image_owner 127.0.0.1/32 scram-sha-256\n' if image_config else ''
        hba.write_text('local all relay_app peer map=ayati_load_runtime\n' + tcp_hba + hba.read_text())
        with (cluster / 'pg_ident.conf').open('a') as ident:
            ident.write(f'\nayati_load_runtime {admin} relay_app\n')
        pg_command = [pg_bin / 'postgres', '-D', cluster, '-k', socket, '-p', env['PGPORT'],
                      '-c', 'listen_addresses=' + ('127.0.0.1' if image_config else ''), '-c', 'shared_buffers=128MB',
                      '-c', 'max_connections=64', '-c', 'max_parallel_workers_per_gather=0']
        lib = pg_bin.parent / 'lib/postgresql'
        if lib.is_dir():
            pg_command += ['-c', 'dynamic_library_path=' + str(lib)]
        output = (evidence / 'postgres.log').open('w')
        pg = subprocess.Popen(scope(profile, 'postgres', pg_command), env=env,
                              stdout=output, stderr=subprocess.STDOUT)
        deadline = time.monotonic() + 30
        while True:
            check = subprocess.run(['psql', '-X', '-Atc', 'SELECT 1'], env=env, capture_output=True, timeout=3)
            if check.returncode == 0:
                break
            if pg.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError('Private PostgreSQL startup failed; inspect postgres.log')
            time.sleep(0.1)
        metadata['postgres_before'] = snapshot(pg.pid)
        verify_limits(metadata['postgres_before'], profile, 'postgres')
        run_checked(['psql', '-X', '-v', 'ON_ERROR_STOP=1'], env, evidence / 'setup.log',
                    f'CREATE ROLE relay_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE;\nCREATE DATABASE {dbname};\n')
        if image_config:
            run_checked(['psql', '-X', '-v', 'ON_ERROR_STOP=1'], env, evidence / 'setup.log', agent_image_resources.role_sql(image_config))
        config = subprocess.check_output(['psql', '-X', '-Atc',
            "SELECT json_build_object('fsync',current_setting('fsync'),'synchronous_commit',current_setting('synchronous_commit'),'full_page_writes',current_setting('full_page_writes'),'server_version',current_setting('server_version'))"], env=env, text=True)
        metadata['postgres_configuration'] = json.loads(config)
        if not all(metadata['postgres_configuration'][key] == 'on' for key in ['fsync', 'synchronous_commit', 'full_page_writes']):
            raise RuntimeError('Private PostgreSQL durability settings are disabled')
        url = f'postgresql://{quote(admin)}@localhost/{dbname}?' + urlencode({'host': str(socket), 'port': env['PGPORT']})
        env.update(RELAY_TEST_DATABASE_URL=url, AYATI_TEST_BIN=str(binary),
                   AYATI_LOAD_PROFILE_FILE=str(profile_file), AYATI_LOAD_POSTGRES_PID=str(pg.pid),
                   AYATI_LOAD_SAMPLES=str(args.samples), AYATI_LOAD_REPEATS=str(args.repeats),
                   AYATI_LOAD_CONCURRENCY=args.concurrency, AYATI_LOAD_CONTEXTS=args.contexts)
        command = ['node', '--test', '--test-concurrency=1', *SUITES[args.suite]]
        save()
        with (evidence / 'run.log').open('w') as log:
            result = subprocess.run(scope(profile, 'driver', command), cwd=ROOT, env=env,
                                    stdout=log, stderr=subprocess.STDOUT)
        metadata['exit_code'] = result.returncode
        metadata['postgres_after'] = snapshot(pg.pid)
        verify_limits(metadata['postgres_after'], profile, 'postgres')
    except KeyboardInterrupt:
        metadata['errors'].append('Rehearsal interrupted')
        metadata['exit_code'] = 130
    except Exception as error:
        metadata['errors'].append(str(error))
        metadata['exit_code'] = 1
    finally:
        if image_config:
            try:
                agent_image_resources.stop_owned(image_config)
            except Exception as error:
                metadata['errors'].append(str(error))
                metadata['exit_code'] = 1
        if pg and pg.poll() is None:
            pg.send_signal(signal.SIGINT)
            try:
                pg.wait(timeout=15)
            except subprocess.TimeoutExpired:
                pg.kill()
                pg.wait()
                metadata['errors'].append('Private PostgreSQL required forced stop')
                metadata['exit_code'] = 1
        stop_scopes(profile)
        if output:
            output.close()
        metadata['finished_utc'] = datetime.now(timezone.utc).isoformat()
        save()
    print('Result:', metadata['exit_code'], '; inspect run.log and result.json', flush=True)
    return metadata['exit_code'] or 0


if __name__ == '__main__':
    raise SystemExit(main())
