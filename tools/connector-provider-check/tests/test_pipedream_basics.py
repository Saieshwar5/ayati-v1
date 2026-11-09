import io
import json
import tempfile
import unittest
from pathlib import Path

from fake_server import FakeServer, load

pipedream = load("pipedream_basics")
CLIENT_ID = "pd_client_id_4b1d"
SECRET = "pd_client_secret_8e2a77"
TOKEN = "pd_access_token_c0ffee"
PREFIX = "/v1/"
ENV = {"PIPEDREAM_CLIENT_ID": CLIENT_ID, "PIPEDREAM_CLIENT_SECRETS": SECRET}
CATALOG = {"apps": [{"id": "gmail", "providers": {"composio": "gmail", "pipedream": "gmail"}, "actions": [
    {"id": "gmail.send_email", "providers": {"composio": {"tool": "GMAIL_SEND_EMAIL", "fields": {"to": "recipient_email"}},
                                             "pipedream": {"tool": "gmail-send-email", "fields": {"to": "to", "subject": "subject", "body": "body"}}}},
    {"id": "gmail.find_emails", "providers": {"pipedream": {"tool": "gmail-find-email", "fields": {"query": "q"}}}},
]}]}


def token(headers, body):
    if body == {"grant_type": "client_credentials", "client_id": CLIENT_ID, "client_secret": SECRET}:
        return 200, {"access_token": TOKEN, "token_type": "Bearer", "expires_in": 3600}
    return 401, {"error": "invalid_client"}


def routes(project="proj_123"):
    root = f"{PREFIX}connect/{project}/"
    return {
        ("POST", PREFIX + "oauth/token"): token,
        ("GET", root + "apps/gmail"): (200, {"data": {"name_slug": "gmail", "name": "Gmail"}}),
        ("GET", root + "components/gmail-send-email"): (200, {"data": {"key": "gmail-send-email", "version": "0.2.1", "configurable_props": [
            {"name": "gmail", "type": "app", "app": "gmail"}, {"name": "to", "type": "string[]"},
            {"name": "subject", "type": "string"}, {"name": "body", "type": "string"},
            {"name": "attachments", "type": "string[]", "optional": True}]}}),
        # The search component renamed its query and now requires a label.
        ("GET", root + "components/gmail-find-email"): (200, {"data": {"key": "gmail-find-email", "version": "0.1.4", "configurable_props": [
            {"name": "gmail", "type": "app"}, {"name": "query", "type": "string"}, {"name": "label", "type": "string"}]}}),
    }


class PipedreamBasics(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.catalog = Path(directory.name) / "catalog.json"
        self.catalog.write_text(json.dumps(CATALOG))

    def check(self, fake, env):
        out = io.StringIO()
        code = pipedream.run(env, out, base=fake.base(PREFIX), catalog_path=self.catalog)
        return code, out.getvalue()

    def assert_nothing_secret_shown(self, output):
        for secret in (CLIENT_ID, SECRET, TOKEN, SECRET[-4:], TOKEN[-4:]):
            self.assertNotIn(secret, output)

    def test_without_a_project_only_the_token_exchange_runs(self):
        with FakeServer(routes()) as fake:
            code, output = self.check(fake, ENV)
        self.assertEqual(output.splitlines(), [
            "credentials set: yes", "token exchange: accepted, expires in 3600 s", "environment: development",
            "project: not set", "catalog entries for pipedream: 1 apps, 2 actions"])
        self.assertEqual(code, 0)
        self.assertEqual([(r["method"], r["path"]) for r in fake.requests], [("POST", "/v1/oauth/token")])
        self.assert_nothing_secret_shown(output)

    def test_with_a_project_each_mapped_app_and_component_is_read(self):
        with FakeServer(routes()) as fake:
            code, output = self.check(fake, ENV | {"PIPEDREAM_PROJECT_ID": "proj_123", "PIPEDREAM_ENVIRONMENT": "production"})
        lines = output.splitlines()
        self.assertIn("environment: production", lines)
        self.assertIn("app gmail (gmail): found", lines)
        self.assertIn("component gmail-send-email (gmail.send_email): ok, version 0.2.1", lines)
        self.assertIn("component gmail-find-email (gmail.find_emails): missing fields q, requires label, query, which Cuckoo does not map, version 0.1.4", lines)
        self.assertEqual(code, 1)
        reads = [r for r in fake.requests if r["method"] == "GET"]
        self.assertEqual(len(reads), 3)
        for read in reads:
            self.assertEqual(read["headers"]["authorization"], "Bearer " + TOKEN)
            self.assertEqual(read["headers"]["x-pd-environment"], "production")
        self.assert_nothing_secret_shown(output)

    def test_refused_credentials_fail(self):
        with FakeServer(routes()) as fake:
            code, output = self.check(fake, ENV | {"PIPEDREAM_CLIENT_SECRETS": "wrong-secret"})
        self.assertIn("token exchange: refused (HTTP 401)", output)
        self.assertNotIn("wrong-secret", output)
        self.assertEqual(code, 1)

    def test_missing_credentials_or_an_unknown_environment_fail_before_any_request(self):
        with FakeServer(routes()) as fake:
            self.assertEqual(self.check(fake, {})[0], 1)
            code, output = self.check(fake, ENV | {"PIPEDREAM_ENVIRONMENT": "staging"})
        self.assertEqual(code, 1)
        self.assertIn("environment: not development or production", output)
        self.assertEqual(fake.requests, [])

    def test_the_shipped_catalog_parses(self):
        apps, actions = pipedream.catalog_entries(json.loads(pipedream.CATALOG.read_text()))
        self.assertIsInstance(apps, list)
        self.assertIsInstance(actions, list)


if __name__ == "__main__":
    unittest.main()
