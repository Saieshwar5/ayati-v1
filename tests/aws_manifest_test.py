"""Deployment/configuration boundaries, independent of AWS credentials and services."""
from copy import deepcopy
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from aws_preflight.manifest import validate
from aws_preflight.templates import invariants, load_templates
from aws_preflight.configuration import controller_environment, stack_parameters


ROOT = Path(__file__).resolve().parents[1]
NOW = datetime(2026, 10, 24, 12, tzinfo=timezone.utc)


def complete_manifest():
    data = json.loads((ROOT / "deploy/aws/trial.example.json").read_text())
    account = "123456789012"
    data.update(account_id=account, owner="test-operator", budget_usd="2.00",
                expires_at=(NOW + timedelta(hours=1)).isoformat())
    data["release"].update(source_commit="a" * 40,
                           image_uri=f"{account}.dkr.ecr.ap-south-1.amazonaws.com/ayati-trial/runtime@sha256:" + "b" * 64)
    data["model"] = {"provider": "openai", "id": "test-model"}
    data["inputs"].update(zone_a="ap-south-1a", zone_b="ap-south-1b", ami_id="ami-" + "a" * 17,
                          certificate_arn=f"arn:aws:acm:ap-south-1:{account}:certificate/" + "a" * 36,
                          runtime_boundary_arn=f"arn:aws:iam::{account}:policy/runtime-boundary",
                          operator_role_arn=f"arn:aws:iam::{account}:role/test-operator", workos_client_id="client_test")
    for name in ("runtime_database_secret_arn", "admin_secret_arn", "workos_secret_arn", "model_secret_arn"):
        data["inputs"][name] = f"arn:aws:secretsmanager:ap-south-1:{account}:secret:{name}-abcdef"
    return data


class ManifestTests(unittest.TestCase):
    def test_complete_configuration_still_cannot_activate_unproven_cloud_code(self):
        result = validate(complete_manifest(), NOW)
        self.assertTrue(result["configuration_complete"], result)
        self.assertFalse(result["deployment_ready"])
        self.assertTrue(result["activation_gates"])

    def test_example_stays_explicitly_incomplete(self):
        result = validate(json.loads((ROOT / "deploy/aws/trial.example.json").read_text()), NOW)
        self.assertTrue(result["schema_ok"], result)
        self.assertIn("budget_usd", result["missing"])
        self.assertIn("release.image_uri", result["missing"])
        self.assertFalse(result["configuration_complete"])

    def test_release_requires_the_guided_permissions_migration(self):
        data = complete_manifest()
        self.assertEqual(data["release"]["schema_version"], 20)
        migration = (ROOT / "src/identity/postgres/migration.rs").read_text()
        self.assertIn("m20261025_000020_guided_permissions", migration)
        data["release"]["schema_version"] = 19
        self.assertFalse(validate(data, NOW)["schema_ok"])

    def test_unknown_secret_fields_are_rejected_without_echoing_them(self):
        data = complete_manifest()
        sentinel = "PRIVATE_CREDENTIAL_SENTINEL"
        data["inputs"][sentinel] = sentinel
        result = validate(data, NOW)
        self.assertFalse(result["schema_ok"])
        self.assertNotIn(sentinel, json.dumps(result))

    def test_wrong_account_credentials_and_mutable_image_tags_fail(self):
        for field, value in [("model_secret_arn", "arn:aws:secretsmanager:ap-south-1:999999999999:secret:key"),
                             ("certificate_arn", "arn:aws:acm:us-east-1:123456789012:certificate/" + "a" * 36)]:
            with self.subTest(field=field):
                data = complete_manifest()
                data["inputs"][field] = value
                self.assertFalse(validate(data, NOW)["schema_ok"])
        data = complete_manifest()
        data["release"]["image_uri"] = data["release"]["image_uri"].split('@')[0] + ":latest"
        self.assertFalse(validate(data, NOW)["schema_ok"])

    def test_secrets_and_zones_cannot_be_reused(self):
        data = complete_manifest()
        data["inputs"]["admin_secret_arn"] = data["inputs"]["runtime_database_secret_arn"]
        self.assertFalse(validate(data, NOW)["schema_ok"])
        data = complete_manifest()
        data["inputs"]["zone_b"] = data["inputs"]["zone_a"]
        self.assertFalse(validate(data, NOW)["schema_ok"])

    def test_wrong_types_are_denied_without_crashing(self):
        for invalid in ([], {}, True, 99):
            data = complete_manifest()
            data["inputs"]["admin_secret_arn"] = invalid
            self.assertFalse(validate(data, NOW)["schema_ok"])

    def test_trial_duration_and_capacity_cannot_expand_silently(self):
        for section, field, value in [("scenario", "active_test_minutes", 16), ("capacity", "executor_count", 5),
                                      ("activation", "executor_count", 1), ("activation", "autoscaling_enabled", True),
                                      ("capacity", "chat_reserve", 4), ("scenario", "agents", True)]:
            with self.subTest(field=field):
                data = complete_manifest()
                data[section][field] = value
                self.assertFalse(validate(data, NOW)["schema_ok"])

    def test_invalid_or_missing_cost_and_cleanup_controls_fail(self):
        for budget in ("NaN", "Infinity", "0", "-1", "1.001", 2, True):
            data = complete_manifest()
            data["budget_usd"] = budget
            self.assertFalse(validate(data, NOW)["schema_ok"], budget)
        for expiry in ("2026-10-24T11:00:00Z", "2026-10-24T13:00:00", "2026-10-27T13:00:00Z"):
            data = complete_manifest()
            data["expires_at"] = expiry
            self.assertFalse(validate(data, NOW)["schema_ok"], expiry)


class TemplateBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.templates = load_templates()

    def test_current_package_preserves_runtime_boundaries(self):
        self.assertEqual(invariants(self.templates), [])
        manifest = complete_manifest()
        parameters = stack_parameters(manifest)
        for name, template in self.templates.items():
            self.assertEqual(set(parameters[name]), set(template["Parameters"]), name)
        settings = controller_environment(manifest)
        self.assertEqual(settings["AYATI_CHAT_EXECUTION"], "pooled")
        self.assertNotIn("RELAY_DATABASE_URL", settings)

    def test_accidental_compute_activation_fails(self):
        for stack, name, field, value in [("controller", "Controllers", "MaxSize", "1"),
                                         ("executors", "Service", "DesiredCount", 1),
                                         ("executors", "Scaling", "MaxCapacity", 2)]:
            with self.subTest(name=name):
                data = deepcopy(self.templates)
                data[stack]["Resources"][name]["Properties"][field] = value
                self.assertTrue(invariants(data))

    def test_core_secret_in_executor_or_public_db_fails(self):
        data = deepcopy(self.templates)
        definitions = data["executors"]["Resources"]["Task"]["Properties"]["ContainerDefinitions"]
        definitions[1]["Secrets"].append({"Name": "RELAY_DATABASE_URL", "ValueFrom": "irrelevant"})
        self.assertTrue(invariants(data))
        data = deepcopy(self.templates)
        data["data"]["Resources"]["Database"]["Properties"]["PubliclyAccessible"] = True
        self.assertTrue(invariants(data))

    def test_broad_executor_iam_and_exposed_control_are_rejected(self):
        data = deepcopy(self.templates)
        policy = data["permissions"]["Resources"]["ExecutorRole"]["Properties"]["Policies"][0]
        policy["PolicyDocument"]["Statement"].append({"Effect": "Allow", "Action": "s3:*", "Resource": "*"})
        self.assertTrue(invariants(data))
        data = deepcopy(self.templates)
        data["network"]["Resources"]["ControllerGroup"]["Properties"]["SecurityGroupIngress"].append({"FromPort": 8766})
        self.assertTrue(invariants(data))


if __name__ == "__main__":
    unittest.main()
