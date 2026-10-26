"""Fail-closed resource verification and owned-scope cleanup boundaries."""
import importlib.util
from pathlib import Path
import tempfile
import subprocess
import sys
import unittest
from unittest.mock import patch, Mock

spec = importlib.util.spec_from_file_location('agent_load_runner',
    Path(__file__).resolve().parents[1] / 'tools/run-agent-load.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class ResourceBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.profile = {'prefix': 'ayati-load-' + 'a' * 16,
            'roles': {'postgres': {'cpu_percent': 100, 'memory_bytes': 536870912, 'tasks': 256}}}
        self.snapshot = {'path': '/user.slice/' + self.profile['prefix'] + '-postgres-' + 'b' * 32 + '.scope',
            'cpu.max': '100000 100000', 'memory.max': '536870912',
            'memory.swap.max': '0', 'pids.max': '256', 'memory.events': 'oom 0\noom_kill 0'}

    def test_missing_or_wrong_resource_limits_cannot_pass(self):
        runner.verify_limits(self.snapshot, self.profile, 'postgres')
        for key, value in [('cpu.max', 'max 100000'), ('memory.max', 'max'),
                           ('memory.swap.max', 'max'), ('pids.max', 'max')]:
            with self.subTest(key=key), self.assertRaises((RuntimeError, ValueError)):
                runner.verify_limits({**self.snapshot, key: value}, self.profile, 'postgres')

    def test_oom_and_foreign_scope_fail_even_if_numeric_limits_match(self):
        for changes in [{'memory.events': 'oom 1\noom_kill 1'},
                        {'path': '/user.slice/shared-postgres.service'}]:
            with self.subTest(changes=changes), self.assertRaises(RuntimeError):
                runner.verify_limits({**self.snapshot, **changes}, self.profile, 'postgres')

    def test_cleanup_refuses_an_unowned_service(self):
        with tempfile.TemporaryDirectory() as directory:
            units = Path(directory) / 'units.txt'
            units.write_text('postgresql.service\n')
            with patch.object(runner.subprocess, 'run') as stop:
                with self.assertRaises(RuntimeError):
                    runner.stop_scopes({**self.profile, 'units_file': str(units)})
                stop.assert_not_called()

    def test_cleanup_targets_only_a_recorded_owned_scope(self):
        with tempfile.TemporaryDirectory() as directory:
            units = Path(directory) / 'units.txt'
            unit = self.profile['prefix'] + '-postgres-' + 'b' * 32 + '.scope'
            units.write_text(unit + '\n')
            with patch.object(runner.subprocess, 'run', side_effect=[
                Mock(returncode=5), Mock(returncode=0, stdout='inactive\n')]) as stop:
                result = runner.stop_scopes({**self.profile, 'units_file': str(units)})
                self.assertEqual(result, [{'unit': unit, 'state': 'inactive', 'stop_exit': 5}])
                self.assertEqual(stop.call_args_list[0].args[0], ['systemctl', '--user', 'stop', unit])
                self.assertEqual(stop.call_count, 2)

    def test_cleanup_checks_every_owned_scope_after_a_stop_timeout(self):
        with tempfile.TemporaryDirectory() as directory:
            units = Path(directory) / 'units.txt'
            names = [self.profile['prefix'] + '-driver-' + digit * 32 + '.scope' for digit in ['b', 'c']]
            units.write_text('\n'.join(names))
            with patch.object(runner.subprocess, 'run', side_effect=[
                subprocess.TimeoutExpired('systemctl', 15), Mock(returncode=0),
                Mock(returncode=0, stdout='inactive\n')]) as stop:
                with self.assertRaisesRegex(RuntimeError, 'cleanup failed'):
                    runner.stop_scopes({**self.profile, 'units_file': str(units)})
                self.assertEqual(stop.call_count, 3)
                self.assertEqual(stop.call_args_list[1].args[0][-1], names[0])

    def test_cleanup_does_not_report_a_still_active_scope_as_stopped(self):
        with tempfile.TemporaryDirectory() as directory:
            units = Path(directory) / 'units.txt'
            units.write_text(self.profile['prefix'] + '-driver-' + 'b' * 32 + '.scope\n')
            with patch.object(runner.subprocess, 'run', side_effect=[
                Mock(returncode=0), Mock(returncode=0, stdout='active\n')]):
                with self.assertRaisesRegex(RuntimeError, 'remains active'):
                    runner.stop_scopes({**self.profile, 'units_file': str(units)})

    def test_invalid_deadlines_are_rejected_before_allocating_test_infrastructure(self):
        for deadline in ['0', '59', '14401']:
            with self.subTest(deadline=deadline), patch.object(sys, 'argv',
                ['run-agent-load.py', '--timeout-seconds', deadline]), \
                patch.object(runner.tempfile, 'mkdtemp') as allocate, \
                patch.object(sys, 'stderr', new=Mock()):
                with self.assertRaises(SystemExit) as error:
                    runner.main()
                self.assertEqual(error.exception.code, 2)
                allocate.assert_not_called()


if __name__ == '__main__':
    unittest.main()
