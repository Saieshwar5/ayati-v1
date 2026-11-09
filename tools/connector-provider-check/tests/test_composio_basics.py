import io
import json
import tempfile
import unittest
from pathlib import Path

from fake_server import FakeServer, load

composio = load("composio_basics")
KEY = "ak_test_project_key_7f3e9a"
PREFIX = "/api/v3.1/"


def tool(slug, properties, required=(), version="20260901_00", deprecated=False):
    return {"slug": slug, "version": version, "is_deprecated": deprecated,
            "input_parameters": {"type": "object", "properties": properties, "required": list(required)}}


TEXT = {"type": "string"}
SEND = tool("GMAIL_SEND_EMAIL", {"recipient_email": TEXT, "subject": TEXT, "body": TEXT, "is_html": {"type": "boolean"},
                                 "cc": {"type": "array"}})
CATALOG = {"apps": [{"id": "gmail", "providers": {"composio": "gmail"}, "actions": [
    {"id": "gmail.send_email", "required": ["to", "subject", "body"], "providers": {"composio": {
        "tool": "GMAIL_SEND_EMAIL", "fields": {"to": "recipient_email", "subject": "subject", "body": "body", "is_html": "is_html"}}}},
    {"id": "gmail.find_emails", "providers": {"composio": {"tool": "GMAIL_FETCH_EMAILS", "fields": {"query": "query", "max_results": "max_results"}}}},
    {"id": "gmail.reply_to_thread", "providers": {"composio": {"tool": "GMAIL_REPLY_TO_THREAD", "fields": {"thread_id": "thread_id"}}}},
]}]}


def routes():
    return {
        ("GET", PREFIX + "auth_configs?limit=1"): (200, {"items": [{"id": "ac_1"}]}),
        ("GET", PREFIX + "auth_configs?limit=50"): (200, {"items": [{"id": "ac_1"}, {"id": "ac_2"}], "total_items": 2}),
        ("GET", "/api/v3/webhook_subscriptions"): (200, {"items": [{"id": "ws_1", "webhook_url": "https://cuckoo.test/hooks/composio"}]}),
        ("GET", PREFIX + "tools/GMAIL_SEND_EMAIL"): (200, SEND),
        # The search lost a mapped field.
        ("GET", PREFIX + "tools/GMAIL_FETCH_EMAILS"): (200, tool("GMAIL_FETCH_EMAILS", {"query": TEXT}, version="20261101_00")),
    }


class ComposioBasics(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.catalog = Path(directory.name) / "catalog.json"
        self.catalog.write_text(json.dumps(CATALOG))

    def check(self, fake, key=KEY):
        out = io.StringIO()
        code = composio.run({"COMPOSIO_API": key}, out, base=fake.base(PREFIX), catalog_path=self.catalog)
        return code, out.getvalue()

    def assert_key_never_shown(self, fake, output, key=KEY):
        for part in (key, key[:8], key[-4:]):
            self.assertNotIn(part, output)
        for request in fake.requests:
            self.assertEqual(request["method"], "GET", "read-only")
            self.assertNotIn(key, request["path"])
            self.assertEqual(request["headers"]["x-api-key"], key)

    def test_reports_the_key_auth_configs_webhook_and_each_tool(self):
        with FakeServer(routes()) as fake:
            code, output = self.check(fake)
        lines = output.splitlines()
        self.assertEqual(lines[:4], ["key accepted: yes", "project key: yes", "auth configs: 2", "webhook subscription: yes"])
        self.assertEqual(lines[4], "tool GMAIL_SEND_EMAIL (gmail.send_email): ok, version 20260901_00")
        self.assertEqual(lines[5], "tool GMAIL_FETCH_EMAILS (gmail.find_emails): offered without some fields, missing fields max_results, version 20261101_00")
        self.assertEqual(lines[6], "tool GMAIL_REPLY_TO_THREAD (gmail.reply_to_thread): not found (HTTP 404)")
        self.assertEqual(json.loads(lines[7].removeprefix("SUMMARY ")), {"ok": 1, "changed": 1, "not_found": 1})
        self.assertEqual(code, 1)
        self.assert_key_never_shown(fake, output)

    def test_a_clean_project_exits_zero_and_no_subscription_reads_no(self):
        clean = {("GET", PREFIX + "tools/GMAIL_FETCH_EMAILS"): (200, tool("GMAIL_FETCH_EMAILS", {"query": TEXT, "max_results": {"type": "integer"}})),
                 ("GET", PREFIX + "tools/GMAIL_REPLY_TO_THREAD"): (200, tool("GMAIL_REPLY_TO_THREAD", {"thread_id": TEXT})),
                 ("GET", PREFIX + "webhook_subscriptions"): (200, {"items": []})}
        with FakeServer(routes() | clean) as fake:
            code, output = self.check(fake)
        self.assertIn("webhook subscription: no", output)
        self.assertEqual(code, 0, output)

    def test_a_personal_key_is_reported_and_fails(self):
        personal = "uak_personal_user_key_91c2"
        with FakeServer(routes()) as fake:
            code, output = self.check(fake, personal)
        self.assertIn("project key: no", output)
        self.assertEqual(code, 1)
        self.assert_key_never_shown(fake, output, personal)

    def test_a_refused_key_stops_before_reading_tools(self):
        with FakeServer(routes() | {("GET", PREFIX + "auth_configs?limit=1"): (401, {"error": "bad key " + KEY})}) as fake:
            code, output = self.check(fake)
        self.assertEqual(output.splitlines()[:2], ["key accepted: no (HTTP 401)", "project key: yes"])
        self.assertEqual(len(fake.requests), 1)
        self.assertEqual(code, 1)
        self.assert_key_never_shown(fake, output)

    def test_no_key_set(self):
        out = io.StringIO()
        self.assertEqual(composio.run({}, out), 1)
        self.assertEqual(out.getvalue(), "key set: no (set COMPOSIO_API)\n")

    def test_comparison_follows_how_cuckoo_reduces_fields(self):
        fields = {"to": "recipient_email", "subject": "subject"}
        self.assertTrue(composio.compare(fields, ["to"], SEND)["offered"])
        # Composio requires a field Cuckoo doesn't map: the action can't be offered.
        strict = tool("GMAIL_SEND_EMAIL", SEND["input_parameters"]["properties"], required=["recipient_email", "cc"])
        result = composio.compare(fields, ["to"], strict)
        self.assertEqual((result["offered"], result["unmapped_required"]), (False, ["cc"]))
        self.assertIn("not offered: Composio requires cc, which Cuckoo does not map", composio.describe(result))
        # A required Cuckoo field that is no longer a plain value.
        nested = tool("GMAIL_SEND_EMAIL", {"recipient_email": {"type": "array"}, "subject": TEXT})
        result = composio.compare(fields, ["to"], nested)
        self.assertEqual((result["offered"], result["not_plain"]), (False, ["recipient_email"]))
        self.assertFalse(composio.compare(fields, [], tool("X", {"recipient_email": TEXT, "subject": TEXT}, deprecated=True))["offered"])

    def test_the_shipped_catalog_names_composio_tools(self):
        actions = list(composio.catalog_actions(json.loads(composio.CATALOG.read_text())))
        tools = [tool for _, tool, _, _ in actions]
        self.assertIn("GMAIL_SEND_EMAIL", tools)
        self.assertTrue(all(fields for _, _, fields, _ in actions))


if __name__ == "__main__":
    unittest.main()
