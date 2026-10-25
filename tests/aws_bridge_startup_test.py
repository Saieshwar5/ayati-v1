"""Real bridge startup/shutdown and fail-closed private configuration, without AWS calls."""
from copy import deepcopy
import http.client
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "integrations"))
from aws_bridge.config import private_json
from aws_bridge.startup import BridgeRuntime, reject_core_secrets, validate


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


class StartupTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.config = {"mode": "worker", "gateway": "https://test.execute-api.ap-south-1.amazonaws.com/trial",
                       "control_port": free_port(), "model_port": free_port(),
                       "control_key_file": str(self.root / "control.key"), "model_key_file": str(self.root / "model.key")}

    def health(self, number):
        client = http.client.HTTPConnection("127.0.0.1", number, timeout=1)
        try:
            client.request("GET", "/health")
            response = client.getresponse()
            response.read()
            return response.status
        finally:
            client.close()

    def launch(self, config=None, extra=None):
        path = self.root / "config.json"
        path.write_text(json.dumps(self.config if config is None else config))
        path.chmod(0o600)
        environment = {key: value for key, value in os.environ.items()
                       if not key.startswith(("AWS_", "RELAY_", "WORKOS_", "WORK_OS_", "OPENAI_", "FIREWORKS_"))}
        environment.update(AWS_EC2_METADATA_DISABLED="true", PYTHONPATH=str(Path(__file__).resolve().parents[1] / "integrations")
                           + os.pathsep + os.environ.get("PYTHONPATH", ""))
        environment.update(extra or {})
        entrypoint = os.environ.get("AYATI_BRIDGE_TEST_ENTRYPOINT")
        command = [entrypoint, "bridge"] if entrypoint else [sys.executable, "-m", "aws_bridge"]
        child = subprocess.Popen([*command, str(path)], env=environment,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(self.stop, child)
        return child

    def stop(self, child):
        if child.poll() is None:
            child.terminate()
        try:
            return child.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.communicate()
            self.fail("Bridge did not stop within its shutdown bound")

    def test_cli_prepares_distinct_private_keys_and_stops_both_listeners(self):
        child = self.launch()
        deadline = time.monotonic() + 5
        while True:
            if child.poll() is not None:
                self.fail("Bridge exited before readiness: " + child.communicate()[1])
            try:
                if all(self.health(self.config[name]) == 204 for name in ("control_port", "model_port")):
                    break
            except OSError:
                pass
            if time.monotonic() > deadline:
                self.fail("Bridge did not start both listeners")
            time.sleep(0.02)
        keys = [Path(self.config[name]).read_text() for name in ("control_key_file", "model_key_file")]
        self.assertNotEqual(*keys)
        for name in ("control_key_file", "model_key_file"):
            self.assertEqual(Path(self.config[name]).stat().st_mode & 0o777, 0o600)
        child.send_signal(signal.SIGTERM)
        stdout, stderr = self.stop(child)
        self.assertEqual(child.returncode, 0, stderr)
        self.assertIn("listeners ready", stdout)
        for key in keys:
            self.assertNotIn(key, stdout + stderr)
        for name in ("control_port", "model_port"):
            with self.assertRaises(OSError):
                self.health(self.config[name])

    def test_cli_rejects_inline_secrets_without_printing_them(self):
        sentinel = "SECRET_CONFIG_SENTINEL"
        child = self.launch({**self.config, "provider_key": sentinel})
        stdout, stderr = child.communicate(timeout=5)
        self.assertEqual(child.returncode, 1)
        self.assertNotIn(sentinel, stdout + stderr)
        self.assertNotIn("ready", stdout)

    def test_cli_refuses_core_credentials_before_readiness(self):
        child = self.launch(extra={"RELAY_DATABASE_URL_FILE": "PRIVATE_DATABASE_SENTINEL"})
        stdout, stderr = child.communicate(timeout=5)
        self.assertEqual(child.returncode, 1)
        self.assertNotIn("PRIVATE_DATABASE_SENTINEL", stdout + stderr)
        self.assertFalse(Path(self.config["control_key_file"]).exists())

    def test_all_core_secret_forms_are_rejected(self):
        for name in ("RELAY_DATABASE_URL", "RELAY_SIMULATION_DATABASE_URL", "WORKOS_API_KEY", "WORK_OS_API_KEY",
                     "OPENAI_API_KEY", "FIREWORKS_API_KEY"):
            for suffix in ("", "_FILE"):
                with self.assertRaises(ValueError):
                    reject_core_secrets({name + suffix: "private"})

    def test_partial_bind_failure_closes_the_first_listener(self):
        with socket.socket() as occupied:
            occupied.bind(("127.0.0.1", self.config["model_port"]))
            occupied.listen()
            with self.assertRaises(OSError):
                with BridgeRuntime(self.config):
                    self.fail("Occupied port must prevent startup")
        with self.assertRaises(OSError):
            self.health(self.config["control_port"])

    def test_private_key_directory_and_reused_credentials_are_denied(self):
        self.root.chmod(0o755)
        with self.assertRaises(ValueError):
            with BridgeRuntime(self.config):
                self.fail("Public key directory accepted")
        self.root.chmod(0o700)
        for name in ("control_key_file", "model_key_file"):
            path = Path(self.config[name])
            path.write_text("a" * 43)
            path.chmod(0o600)
        with self.assertRaises(ValueError):
            with BridgeRuntime(self.config):
                self.fail("Reused credentials accepted")

    def test_config_requires_private_regular_file(self):
        path = self.root / "bad.json"
        path.write_text(json.dumps(self.config))
        path.chmod(0o644)
        with self.assertRaises(ValueError):
            private_json(path)
        path.chmod(0o600)
        alias = self.root / "alias.json"
        alias.symlink_to(path)
        with self.assertRaises(OSError):
            private_json(alias)

    def test_configuration_cannot_expose_listeners_or_share_endpoints(self):
        for field, value in (("bind", "0.0.0.0"), ("control_port", True), ("control_port", 80),
                             ("gateway", "http://127.0.0.1:8766"), ("control_key_file", "relative.key"),
                             ("model_port", self.config["control_port"]),
                             ("model_key_file", self.config["control_key_file"])):
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate({**self.config, field: value})

    def test_controller_config_requires_matching_infrastructure_and_private_backend(self):
        config = {"mode": "controller", "listen_port": free_port(), "control_url": "http://127.0.0.1:8766",
                  "bootstrap_file": str(self.root / "operator.key"), "model_info_file": str(self.root / "model.json"),
                  "role_arn": "arn:aws:iam::123456789012:role/ayati-test-executor",
                  "cluster_arn": "arn:aws:ecs:ap-south-1:123456789012:cluster/ayati-test",
                  "release_arn": "arn:aws:ecs:ap-south-1:123456789012:task-definition/ayati-test:1"}
        self.assertEqual(validate(config), config)
        for field, value in (("control_url", "http://0.0.0.0:8766"), ("bootstrap_file", "relative"),
                             ("release_arn", config["release_arn"].replace("123456789012", "999999999999")),
                             ("role_arn", config["role_arn"] + "/nested")):
            changed = deepcopy(config)
            changed[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                validate(changed)


if __name__ == "__main__":
    unittest.main()
