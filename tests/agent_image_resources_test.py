"""Container cleanup must never act on an unowned name, label or image."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('agent_image_resources',
    Path(__file__).resolve().parents[1] / 'tools/agent_image_resources.py')
images = importlib.util.module_from_spec(spec)
spec.loader.exec_module(images)


class ImageOwnershipTests(unittest.TestCase):
    def fixture(self, directory):
        config = {'session': 'a' * 32, 'image': 'sha256:' + 'c' * 64,
                  'containers_file': str(Path(directory) / 'owned')}
        name = 'ayati-image-' + config['session'] + '-executor-' + 'b' * 32
        Path(config['containers_file']).write_text(name + '\n')
        return config, name

    def test_mutable_tags_are_refused_before_docker_is_called(self):
        with patch.object(images.subprocess, 'run') as run:
            with self.assertRaises(RuntimeError):
                images.prepare(Path('/unused'), 'ayati:latest')
            run.assert_not_called()

    def test_foreign_name_is_never_inspected_or_removed(self):
        with tempfile.TemporaryDirectory() as directory:
            config, _ = self.fixture(directory)
            Path(config['containers_file']).write_text('production-postgres\n')
            with patch.object(images.subprocess, 'run') as run:
                with self.assertRaises(RuntimeError): images.stop_owned(config)
                run.assert_not_called()

    def test_foreign_label_or_image_is_never_removed(self):
        with tempfile.TemporaryDirectory() as directory:
            config, name = self.fixture(directory)
            for changes in [{'Image': 'sha256:' + 'd' * 64},
                            {'Config': {'Labels': {'ayati.test.session': 'foreign'}}}]:
                info = {'Image': config['image'], 'Config': {'Labels': {'ayati.test.session': config['session']}}, **changes}
                with patch.object(images.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, json.dumps([info]), '')) as run:
                    with self.assertRaises(RuntimeError): images.stop_owned(config)
                    self.assertEqual(run.call_count, 1)
                    self.assertEqual(run.call_args.args[0], ['docker', 'container', 'inspect', name])

    def test_only_an_exact_owned_resource_can_be_removed(self):
        with tempfile.TemporaryDirectory() as directory:
            config, name = self.fixture(directory)
            info = {'Image': config['image'], 'Config': {'Labels': {'ayati.test.session': config['session']}}}
            with patch.object(images.subprocess, 'run', side_effect=[
                    subprocess.CompletedProcess([], 0, json.dumps([info]), ''),
                    subprocess.CompletedProcess([], 0)]) as run:
                images.stop_owned(config)
                self.assertEqual(run.call_args.args[0], ['docker', 'rm', '--force', name])

    def test_daemon_denial_cannot_be_mistaken_for_absence(self):
        with tempfile.TemporaryDirectory() as directory:
            config, _ = self.fixture(directory)
            with patch.object(images.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1, '', 'permission denied')) as run:
                with self.assertRaises(RuntimeError): images.stop_owned(config)
                self.assertEqual(run.call_count, 1)
