"""Local-only restore boundaries and failure recovery; no cloud credentials."""
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import workspace_restore as restore


class ConnectionTests(unittest.TestCase):
    def test_only_explicit_local_simulation_connections_are_accepted(self):
        with patch.dict(os.environ, {'PGSERVICE': 'foreign', 'PGOPTIONS': '-c role=other',
                                     'PGDATABASE': 'normal'}, clear=False):
            value = restore.connection('postgresql://test:fixture@localhost/relay_simulation_test?port=55439')
        self.assertEqual(value['PGDATABASE'], 'relay_simulation_test')
        self.assertEqual(value['PGHOST'], 'localhost')
        self.assertEqual(value['PGUSER'], 'test')
        self.assertNotIn('PGSERVICE', value)
        self.assertNotIn('PGOPTIONS', value)
        self.assertEqual(value['PGPASSFILE'], '/dev/null')
        self.assertEqual(value['PGCLIENTENCODING'], 'UTF8')

    def test_remote_normal_and_ambiguous_options_are_rejected(self):
        for value in [
            'postgresql://host.example/relay_simulation_test',
            'postgresql://localhost/production',
            'postgresql://localhost/relay_simulation_test?host=remote.example',
            'postgresql://localhost/relay_simulation_test?options=',
            'postgresql://localhost/relay_simulation_test?port=1&port=2',
            'postgresql://localhost/relay_simulation_test?port=70000',
            'postgresql://localhost/relay_simulation_test#extra',
            'postgresql://localhost/relay_simulation_test?service=hidden',
            'postgresql://localhost/relay_simulation_test?host=/missing/socket/directory',
        ]:
            with self.subTest(value=value), self.assertRaises(restore.RestoreError):
                restore.connection(value)

    def test_schema_names_cannot_be_patterns_or_sql(self):
        for value in ['*', 'other', '../relay_data', 'relay_test; DROP TABLE x', 'relay_a.b', '']:
            with self.subTest(value=value), self.assertRaises(restore.RestoreError):
                restore.schema_name(value)
        self.assertEqual(restore.schema_name('relay_worker_123'), 'relay_worker_123')


class CommandTests(unittest.TestCase):
    def test_large_stdin_and_stdout_are_streamed_without_pipe_deadlock(self):
        data = b'abcd' * 32768
        value = restore.command([sys.executable, '-c',
                                 'import sys; sys.stdout.buffer.write(sys.stdin.buffer.read().upper())'],
                                os.environ.copy(), data)
        self.assertEqual(value, data.upper())

    def test_output_is_stopped_at_the_bound(self):
        with patch.object(restore, 'MAX_BYTES', 1024), self.assertRaisesRegex(restore.RestoreError, 'bound'):
            restore.command([sys.executable, '-c', "print('x' * 65536)"], os.environ.copy())

    def test_failed_command_does_not_disclose_stderr_or_stdout(self):
        with self.assertRaises(restore.RestoreError) as error:
            restore.command([sys.executable, '-c',
                             "import sys; print('PRIVATE_ROW'); sys.stderr.write('PRIVATE_PASSWORD'); sys.exit(4)"],
                            os.environ.copy())
        self.assertNotIn('PRIVATE_', str(error.exception))

    def test_timeout_kills_the_owned_child(self):
        with tempfile.TemporaryDirectory() as temporary:
            pid = Path(temporary) / 'pid'
            script = f'import os,time; open({str(pid)!r}, "w").write(str(os.getpid())); time.sleep(5)'
            started = time.monotonic()
            with patch.object(restore, 'COMMAND_SECONDS', 0.2), self.assertRaises(restore.RestoreError):
                restore.command([sys.executable, '-c', script], os.environ.copy())
            self.assertLess(time.monotonic() - started, 2)
            self.assertTrue(pid.exists())
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid.read_text()), 0)


class ObjectFixture:
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / 'source'
        self.source.mkdir(mode=0o700)
        self.content = b'Private recorded chat attachment\n'
        self.record = {'workspace': str(uuid.uuid4()), 'id': str(uuid.uuid4()),
                       'size': len(self.content), 'checksum': hashlib.sha256(self.content).hexdigest()}
        self.file = self.source / self.record['workspace'] / 'objects' / self.record['id']
        self.file.parent.mkdir(parents=True)
        self.file.write_bytes(self.content)


class FileTests(ObjectFixture, unittest.TestCase):
    def test_copy_includes_only_recorded_objects_with_private_permissions(self):
        (self.source / '.env').write_text('DO_NOT_COPY_CONFIGURATION')
        (self.file.parent / 'orphan').write_text('DO_NOT_COPY_ORPHAN')
        target = self.root / 'copy'
        restore.copy_objects(self.source, target, [self.record])
        copied = target / self.record['workspace'] / 'objects' / self.record['id']
        self.assertEqual(copied.read_bytes(), self.content)
        self.assertEqual(stat.S_IMODE(copied.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o700)
        self.assertEqual([p.name for p in target.rglob('*') if p.is_file()], [self.record['id']])
        self.assertTrue((self.source / '.env').exists())

    def test_missing_corrupt_and_wrong_size_objects_fail(self):
        for content in [b'changed', b'x' * len(self.content)]:
            self.file.write_bytes(content)
            with self.assertRaises(restore.RestoreError):
                restore.verify_objects(self.source, [self.record])
        self.file.unlink()
        with self.assertRaises(restore.RestoreError):
            restore.verify_objects(self.source, [self.record])

    def test_symbolic_file_and_parent_paths_are_not_followed(self):
        elsewhere = self.root / 'elsewhere'
        elsewhere.write_bytes(self.content)
        self.file.unlink()
        self.file.symlink_to(elsewhere)
        with self.assertRaises(restore.RestoreError):
            restore.verify_objects(self.source, [self.record])
        self.file.unlink()
        self.file.write_bytes(self.content)
        directory = self.source / self.record['workspace']
        moved = self.root / 'moved'
        directory.rename(moved)
        directory.symlink_to(moved, target_is_directory=True)
        with self.assertRaises(restore.RestoreError):
            restore.verify_objects(self.source, [self.record])

    def test_root_symlinks_and_non_regular_objects_fail_without_blocking(self):
        link = self.root / 'linked-source'
        link.symlink_to(self.source, target_is_directory=True)
        with self.assertRaises(restore.RestoreError):
            restore.verify_objects(link, [self.record])
        self.file.unlink()
        os.mkfifo(self.file)
        started = time.monotonic()
        with self.assertRaises(restore.RestoreError):
            restore.verify_objects(self.source, [self.record])
        self.assertLess(time.monotonic() - started, 1)

    def test_unsafe_identifiers_and_changed_stream_size_fail(self):
        for key in ['workspace', 'id']:
            record = {**self.record, key: '../escape'}
            with self.assertRaises(restore.RestoreError):
                restore.object_path(self.source, record)
        for content in [b'1234', b'12']:
            with self.assertRaises(restore.RestoreError):
                restore.object_bytes(io.BytesIO(content), 3)

    def test_object_manifest_rejects_unbounded_or_invalid_records(self):
        cases = [[{**self.record, 'size': value}] for value in [0, -1, True, 10485761]]
        cases += [[{**self.record, 'checksum': 'bad'}], [self.record] * 257, {}]
        for records in cases:
            with self.subTest(records=str(records)[:80]), patch.object(restore, 'sql', return_value=json.dumps(records).encode()), self.assertRaises(restore.RestoreError):
                restore.objects({}, 'public')


class RehearsalTests(ObjectFixture, unittest.TestCase):
    def setUp(self):
        super().setUp()
        self.env = {'PGDATABASE': 'relay_simulation_source'}
        self.output = self.root / 'rehearsal'
        self.before = {'tables': [{'table': 'relay_chat_jobs', 'rows': 1, 'sha256': 'a' * 64}],
                       'sequences': [{'sequence': 'messages', 'state': '10|t'}], 'schema_version': 30}
        self.calls = []

    def fake_command(self, args, env, data=None):
        self.calls.append((args, env.copy()))
        if args[0] == 'pg_dump':
            Path(next(a[7:] for a in args if a.startswith('--file='))).write_bytes(b'private dump')
        return b''

    def run_rehearsal(self, keep=False, snapshots=None, command=None):
        with patch.object(restore, 'snapshot', side_effect=snapshots, return_value=self.before), \
             patch.object(restore, 'objects', return_value=[self.record]), \
             patch.object(restore, 'command', side_effect=command or self.fake_command), \
             patch.object(restore, 'sql', return_value=b''):
            return restore.rehearse(self.env, 'relay_fixture', self.source, self.output, keep)

    def test_success_retains_private_evidence_and_drops_only_its_new_database(self):
        result = self.run_rehearsal()
        self.assertTrue(result['passed'])
        self.assertTrue(result['source_unchanged'])
        self.assertFalse(result['target_retained'])
        drop = next(args for args, _ in self.calls if args[0] == 'dropdb')
        self.assertEqual(drop[-1], result['target_database'])
        self.assertNotEqual(drop[-1], self.env['PGDATABASE'])
        for name in ['manifest.json', 'verification.json', 'workspace.dump']:
            self.assertEqual(stat.S_IMODE((self.output / name).stat().st_mode), 0o600)
        self.assertEqual(self.file.read_bytes(), self.content)

    def test_keep_target_is_explicit_and_only_preserves_a_verified_target(self):
        result = self.run_rehearsal(keep=True)
        self.assertTrue(result['target_retained'])
        self.assertFalse(any(args[0] == 'dropdb' for args, _ in self.calls))

    def test_failed_restore_drops_owned_target_even_when_keep_was_requested(self):
        def fail(args, env, data=None):
            result = self.fake_command(args, env, data)
            if args[0] == 'pg_restore':
                raise restore.RestoreError('Controlled restore failure')
            return result
        with self.assertRaises(restore.RestoreError):
            self.run_rehearsal(keep=True, command=fail)
        report = json.loads((self.output / 'verification.json').read_text())
        self.assertFalse(report['passed'])
        self.assertFalse(report['target_retained'])
        self.assertTrue(any(args[0] == 'dropdb' for args, _ in self.calls))

    def test_source_drift_stops_before_creating_a_target(self):
        with self.assertRaisesRegex(restore.RestoreError, 'Source changed'):
            self.run_rehearsal(snapshots=[self.before, {**self.before, 'tables': []}])
        self.assertFalse(any(args[0] == 'createdb' for args, _ in self.calls))
        self.assertFalse(json.loads((self.output / 'verification.json').read_text())['passed'])

    def test_cleanup_failure_is_not_reported_as_success(self):
        def fail(args, env, data=None):
            result = self.fake_command(args, env, data)
            if args[0] == 'dropdb':
                raise restore.RestoreError('Controlled cleanup failure')
            return result
        with self.assertRaises(restore.RestoreError):
            self.run_rehearsal(command=fail)
        report = json.loads((self.output / 'verification.json').read_text())
        self.assertFalse(report['passed'])
        self.assertTrue(report['target_retained'])
        self.assertIn('cleanup_error', report)

    def test_existing_output_and_source_subdirectories_are_not_modified(self):
        self.output.mkdir()
        canary = self.output / 'keep'
        canary.write_text('Keep existing output')
        with self.assertRaises(restore.RestoreError):
            self.run_rehearsal()
        self.assertEqual(canary.read_text(), 'Keep existing output')
        self.output = self.source / 'unsafe-backup'
        with self.assertRaises(restore.RestoreError):
            self.run_rehearsal()
        self.assertFalse(self.output.exists())

    def test_cli_requires_quiescence_before_any_database_or_file_work(self):
        completed = subprocess.run([sys.executable, restore.__file__, '--schema', 'relay_fixture',
                                    '--files-root', str(self.source), '--output', str(self.output)],
                                   env={**os.environ, 'RELAY_TEST_DATABASE_URL': ''}, capture_output=True,
                                   text=True, timeout=5)
        self.assertEqual(completed.returncode, 1)
        self.assertFalse(json.loads(completed.stdout)['passed'])
        self.assertIn('source-quiesced', completed.stdout)
        self.assertFalse(self.output.exists())


if __name__ == '__main__':
    unittest.main()
