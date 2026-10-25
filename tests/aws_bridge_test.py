"""Infrastructure identity, signing boundaries and actual incremental proxy streaming."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "integrations"))
from aws_bridge.config import cloud_identity, gateway_url, local_origin, private_json, private_token
from aws_bridge.server import start_server
from aws_bridge.transport import TaskVerifier, Transport

ACCOUNT = "123456789012"
ROLE = f"arn:aws:iam::{ACCOUNT}:role/ayati-test-executor"
CLUSTER = f"arn:aws:ecs:ap-south-1:{ACCOUNT}:cluster/ayati-test"
RELEASE = f"arn:aws:ecs:ap-south-1:{ACCOUNT}:task-definition/ayati-test:1"
TASK_ID = "a" * 32
PRINCIPAL = f"arn:aws:sts::{ACCOUNT}:assumed-role/ayati-test-executor/{TASK_ID}"
TASK = f"arn:aws:ecs:ap-south-1:{ACCOUNT}:task/ayati-test/{TASK_ID}"


class BridgeTests(unittest.TestCase):
    def test_infrastructure_identity_cannot_be_self_declared_or_cross_scope(self):
        self.assertEqual(cloud_identity(PRINCIPAL, ROLE, CLUSTER), (TASK, TASK_ID))
        for principal in (ROLE, PRINCIPAL.replace(ACCOUNT, "999999999999"), PRINCIPAL.replace("-executor/", "-other/"), PRINCIPAL[:-1] + "Z"):
            with self.assertRaises((ValueError, PermissionError)):
                cloud_identity(principal, ROLE, CLUSTER)

    def test_verifier_checks_real_task_release_role_and_state(self):
        session = Mock()
        ecs = session.client.return_value
        task = {"taskArn": TASK, "clusterArn": CLUSTER, "lastStatus": "RUNNING", "taskDefinitionArn": RELEASE}
        ecs.describe_tasks.return_value = {"tasks": [task]}
        ecs.describe_task_definition.return_value = {"taskDefinition": {"taskRoleArn": ROLE}}
        verifier = TaskVerifier(ROLE, CLUSTER, RELEASE, session=session)
        self.assertEqual(verifier.verify(PRINCIPAL), TASK)
        for field, wrong in (("lastStatus", "STOPPED"), ("taskDefinitionArn", RELEASE + "2"), ("taskArn", TASK + "1")):
            ecs.describe_tasks.return_value = {"tasks": [{**task, field: wrong}]}
            with self.assertRaises(PermissionError):
                verifier.verify(PRINCIPAL)
        ecs.describe_tasks.return_value = {"tasks": [task]}
        ecs.describe_task_definition.return_value = {"taskDefinition": {"taskRoleArn": ROLE + "-other"}}
        with self.assertRaises(PermissionError):
            verifier.verify(PRINCIPAL)

    def test_only_exact_tls_gateway_can_be_signed(self):
        for value in ("http://example.com", "https://example.com/trial", "https://abc.execute-api.ap-south-1.amazonaws.com/trial?key=x"):
            with self.assertRaises(ValueError):
                gateway_url(value)
        transport = Transport("https://abc.execute-api.ap-south-1.amazonaws.com/trial", session=Mock())
        with self.assertRaises(ValueError):
            transport.request("POST", "https://attacker.invalid/v1/executors/register", signed=True)

    def test_task_local_bootstrap_is_private_and_not_shared_with_controller(self):
        with tempfile.TemporaryDirectory() as root:
            one, two = Path(root) / "one", Path(root) / "two"
            self.assertNotEqual(private_token(one), private_token(two))
            self.assertEqual(one.stat().st_mode & 0o777, 0o600)
            self.assertEqual(private_token(one), private_token(one))
            one.chmod(0o644)
            with self.assertRaises(ValueError):
                private_token(one)

    def test_private_configuration_refuses_links_fifos_and_oversized_files(self):
        import os
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            secret = root / "private.json"
            secret.write_text('{}')
            secret.chmod(0o600)
            self.assertEqual(private_json(secret), {})
            link = root / "link"
            link.symlink_to(secret)
            with self.assertRaises(OSError):
                private_json(link)
            fifo = root / "fifo"
            os.mkfifo(fifo, 0o600)
            with self.assertRaises(ValueError):
                private_json(fifo)
            secret.write_text(' ' * 16385)
            with self.assertRaises(ValueError):
                private_json(secret)

    def test_controller_destinations_are_fixed_loopback_origins(self):
        self.assertEqual(local_origin("http://127.0.0.1:8766/"), "http://127.0.0.1:8766")
        for value in ("https://127.0.0.1:8766", "http://127.0.0.1", "http://localhost:8766",
                      "http://user@127.0.0.1:8766", "http://127.0.0.1:8766/private",
                      "http://127.0.0.1:8766?secret=x"):
            with self.assertRaises(ValueError):
                local_origin(value)

    def test_controller_rebuilds_registration_and_denies_direct_internal_routes(self):
        upstream = Mock(status=200, headers={"Content-Type": "application/json"})
        upstream.read1.side_effect = [b'{"registered":true}', b""]
        transport = Mock()
        transport.request.return_value = upstream
        verifier = Mock(role=ROLE, cluster=CLUSTER, release=RELEASE)
        server = start_server(("127.0.0.1", 0), mode="controller", local_key="controller-private",
                              verifier=verifier, transport=transport, model_info=Path("unused"))
        import urllib3
        client = urllib3.PoolManager()
        url = f"http://127.0.0.1:{server.server_port}"
        registration = {"executor": "a", "incarnation": "b", "token": "c", "pooled": True}
        try:
            response = client.request("POST", url + "/v1/executors/register", body=json.dumps(registration),
                                      headers={"x-ayati-verified-principal": PRINCIPAL, "Content-Type": "application/json"})
            self.assertEqual(response.status, 200)
            arguments = transport.request.call_args
            self.assertEqual(arguments.args[1], "http://127.0.0.1:8766/v1/executors/register-cloud")
            self.assertEqual(json.loads(arguments.args[2])["task_arn"], TASK)
            self.assertEqual(arguments.args[3]["Authorization"], "Bearer controller-private")
            response = client.request("POST", url + "/v1/executors/register-cloud", body=b"{}",
                                      headers={"x-ayati-verified-principal": PRINCIPAL})
            self.assertEqual(response.status, 403)
            response = client.request("POST", url + "/v1/executors/register", body=b"{}")
            self.assertEqual(response.status, 403)
        finally:
            server.shutdown(); server.server_close()


if __name__ == "__main__":
    unittest.main()
