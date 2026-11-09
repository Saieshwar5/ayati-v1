"""Composio check: is the key accepted, is it a project key, and does every tool
Cuckoo's catalog names still take the mapped fields on REST v3.1? Also counts the
project's auth configs and says whether a webhook subscription exists.

Reads COMPOSIO_API from the environment and never prints it or any part of it.
Read-only: only GET requests. Python standard library only.
Run: python3 -I composio_basics.py"""
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

OFFICIAL = "https://backend.composio.dev/api/v3.1/"
CATALOG = Path(__file__).resolve().parents[2] / "src/identity/connectors/catalog.json"
PLAIN = {"string", "integer", "number", "boolean"}
# Webhook subscriptions are listed under v3.1 or, on older projects, v3.
WEBHOOKS = ["webhook_subscriptions", "../v3/webhook_subscriptions"]


class Api:
    """GET requests with the key in a header, never in a URL or a message."""

    def __init__(self, base, key, timeout=20):
        self.base, self.key, self.timeout = base, key, timeout
        loopback = urllib.parse.urlsplit(base).hostname == "127.0.0.1"
        # Local fakes in tests bypass any proxy; real runs use HTTPS_PROXY if set.
        handlers = [urllib.request.ProxyHandler({})] if loopback else []
        self.opener = urllib.request.build_opener(*handlers)

    def get(self, path):
        """(HTTP status, JSON body or None). Network failures are status 0."""
        request = urllib.request.Request(urllib.parse.urljoin(self.base, path),
                                         headers={"x-api-key": self.key, "accept": "application/json"})
        try:
            with self.opener.open(request, timeout=self.timeout) as response:
                return response.status, json.loads(response.read(512 * 1024) or b"null")
        except urllib.error.HTTPError as error:
            return error.code, None
        except (urllib.error.URLError, OSError, ValueError):
            return 0, None


def project_key(key):
    """Cuckoo refuses personal user keys (uak_); only the yes/no is reported."""
    return not key.startswith("uak_")


def catalog_actions(catalog):
    """(action id, Composio tool, {Cuckoo field: Composio field}, [Cuckoo-required])."""
    for app in catalog["apps"]:
        for action in app["actions"]:
            binding = action.get("providers", {}).get("composio")
            if binding:
                yield action["id"], binding["tool"], binding["fields"], action.get("required", [])


def compare(fields, required, tool):
    """How a tool's input schema matches Cuckoo's mapping, as Cuckoo reduces it
    (src/identity/connectors/curated.rs): mapped fields must exist and be plain
    values; every field Composio requires must be mapped."""
    schema = tool.get("input_parameters") or {}
    properties = schema.get("properties") or {}
    mapped = set(fields.values())
    missing = sorted(p for p in mapped if p not in properties)
    not_plain = sorted(p for p in mapped if p in properties and properties[p].get("type") not in PLAIN)
    unmapped = sorted(p for p in schema.get("required") or [] if p not in mapped)
    lost = sorted(c for c in required if fields[c] in missing or fields[c] in not_plain)
    return {
        "version": tool.get("version"),
        "deprecated": tool.get("is_deprecated") is True,
        "missing": missing,
        "not_plain": not_plain,
        "unmapped_required": unmapped,
        "offered": not unmapped and not lost and tool.get("is_deprecated") is not True,
    }


def describe(result):
    """One plain line for a tool's comparison."""
    if not result["offered"]:
        reasons = []
        if result["deprecated"]:
            reasons.append("deprecated")
        if result["unmapped_required"]:
            reasons.append("Composio requires " + ", ".join(result["unmapped_required"]) + ", which Cuckoo does not map")
        if not reasons:
            reasons.append("a field Cuckoo requires is missing or not a plain value")
        head = "not offered: " + "; ".join(reasons)
    elif result["missing"] or result["not_plain"]:
        head = "offered without some fields"
    else:
        head = "ok"
    extra = []
    if result["missing"]:
        extra.append("missing fields " + ", ".join(result["missing"]))
    if result["not_plain"]:
        extra.append("not plain values " + ", ".join(result["not_plain"]))
    return ", ".join([head] + extra) + f", version {result['version']}"


def count_auth_configs(api):
    total, cursor = 0, None
    for _ in range(20):
        status, body = api.get("auth_configs?limit=50" + (f"&cursor={urllib.parse.quote(cursor)}" if cursor else ""))
        if status != 200 or not isinstance(body, dict):
            return None
        if isinstance(body.get("total_items"), int):
            return body["total_items"]
        total += len(body.get("items") or [])
        cursor = body.get("next_cursor")
        if not cursor:
            return total
    return total


def webhook_subscription(api):
    """'yes', 'no' or 'unknown' (no listing endpoint answered)."""
    for path in WEBHOOKS:
        status, body = api.get(path)
        if status == 200 and isinstance(body, dict):
            return "yes" if body.get("items") else "no"
    return "unknown"


def run(env, out, base=OFFICIAL, catalog_path=CATALOG):
    """Prints the report to `out`; returns the exit code (1 on any problem)."""
    key = (env.get("COMPOSIO_API") or "").strip()
    if not key:
        print("key set: no (set COMPOSIO_API)", file=out)
        return 1
    api = Api(base, key)
    problems = 0
    status, _ = api.get("auth_configs?limit=1")
    accepted = status == 200
    print(f"key accepted: {'yes' if accepted else 'no'}" + ("" if accepted else f" (HTTP {status or 'no answer'})"), file=out)
    print(f"project key: {'yes' if project_key(key) else 'no'}", file=out)
    problems += (not accepted) + (not project_key(key))
    if not accepted:
        return 1
    configs = count_auth_configs(api)
    print(f"auth configs: {configs if configs is not None else 'unknown'}", file=out)
    print(f"webhook subscription: {webhook_subscription(api)}", file=out)
    catalog = json.loads(Path(catalog_path).read_text())
    summary = {"ok": 0, "changed": 0, "not_found": 0}
    for action, tool, fields, required in catalog_actions(catalog):
        status, body = api.get("tools/" + urllib.parse.quote(tool))
        if status != 200 or not isinstance(body, dict):
            print(f"tool {tool} ({action}): not found (HTTP {status or 'no answer'})", file=out)
            summary["not_found"] += 1
            continue
        result = compare(fields, required, body)
        line = describe(result)
        print(f"tool {tool} ({action}): {line}", file=out)
        summary["ok" if line.startswith("ok") else "changed"] += 1
    problems += summary["changed"] + summary["not_found"]
    print("SUMMARY " + json.dumps(summary), file=out)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(run(os.environ, sys.stdout))
