"""Pipedream check: are the client credentials accepted, and, when a project is
set, does each app and action component Cuckoo's catalog maps to Pipedream exist
with the mapped fields?

Reads PIPEDREAM_CLIENT_ID, PIPEDREAM_CLIENT_SECRETS, PIPEDREAM_PROJECT_ID
(optional) and PIPEDREAM_ENVIRONMENT (development unless set) from the
environment; never prints them or the access token, or any part of them.
Read-only besides exchanging the credentials for a token. Python standard
library only. Run: python3 -I pipedream_basics.py"""
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

OFFICIAL = "https://api.pipedream.com/v1/"
CATALOG = Path(__file__).resolve().parents[2] / "src/identity/connectors/catalog.json"
ENVIRONMENTS = {"development", "production"}


class Api:
    """JSON requests; credentials and tokens stay in bodies and headers."""

    def __init__(self, base, timeout=20):
        self.base, self.timeout, self.token, self.environment = base, timeout, None, None
        loopback = urllib.parse.urlsplit(base).hostname == "127.0.0.1"
        # Local fakes in tests bypass any proxy; real runs use HTTPS_PROXY if set.
        handlers = [urllib.request.ProxyHandler({})] if loopback else []
        self.opener = urllib.request.build_opener(*handlers)

    def send(self, path, body=None):
        """(HTTP status, JSON body or None). Network failures are status 0."""
        headers = {"accept": "application/json"}
        if self.token:
            headers["authorization"] = "Bearer " + self.token
        if self.environment:
            headers["x-pd-environment"] = self.environment
        data = None
        if body is not None:
            data, headers["content-type"] = json.dumps(body).encode(), "application/json"
        request = urllib.request.Request(urllib.parse.urljoin(self.base, path), data=data, headers=headers,
                                         method="POST" if body is not None else "GET")
        try:
            with self.opener.open(request, timeout=self.timeout) as response:
                return response.status, json.loads(response.read(512 * 1024) or b"null")
        except urllib.error.HTTPError as error:
            return error.code, None
        except (urllib.error.URLError, OSError, ValueError):
            return 0, None


def catalog_entries(catalog):
    """Apps ([(app id, Pipedream app)]) and actions ([(action id, component,
    {Cuckoo field: Pipedream prop})]) the catalog maps to Pipedream."""
    apps, actions = [], []
    for app in catalog["apps"]:
        if "pipedream" in app.get("providers", {}):
            apps.append((app["id"], app["providers"]["pipedream"]))
        for action in app["actions"]:
            binding = action.get("providers", {}).get("pipedream")
            if binding:
                actions.append((action["id"], binding["tool"], binding["fields"]))
    return apps, actions


def compare(fields, component):
    """Mapped props the component no longer has, and props it requires that
    Cuckoo doesn't map (app sign-in props are the connected account's)."""
    props = component.get("configurable_props") or []
    names = {p.get("name") for p in props}
    mapped = set(fields.values())
    missing = sorted(p for p in mapped if p not in names)
    unmapped = sorted(p.get("name") for p in props
                      if p.get("type") != "app" and not p.get("optional") and "default" not in p
                      and p.get("name") not in mapped)
    return {"version": component.get("version"), "missing": missing, "unmapped_required": unmapped}


def describe(result):
    parts = []
    if result["missing"]:
        parts.append("missing fields " + ", ".join(result["missing"]))
    if result["unmapped_required"]:
        parts.append("requires " + ", ".join(result["unmapped_required"]) + ", which Cuckoo does not map")
    return (", ".join(parts) or "ok") + f", version {result['version']}"


def run(env, out, base=OFFICIAL, catalog_path=CATALOG):
    """Prints the report to `out`; returns the exit code (1 on any problem)."""
    client_id = (env.get("PIPEDREAM_CLIENT_ID") or "").strip()
    secret = (env.get("PIPEDREAM_CLIENT_SECRETS") or "").strip()
    project = (env.get("PIPEDREAM_PROJECT_ID") or "").strip()
    environment = (env.get("PIPEDREAM_ENVIRONMENT") or "development").strip()
    print(f"credentials set: {'yes' if client_id and secret else 'no'}", file=out)
    if not (client_id and secret):
        return 1
    if environment not in ENVIRONMENTS:
        print("environment: not development or production", file=out)
        return 1
    api = Api(base)
    status, body = api.send("oauth/token", {"grant_type": "client_credentials",
                                            "client_id": client_id, "client_secret": secret})
    token = body.get("access_token") if isinstance(body, dict) else None
    if status != 200 or not isinstance(token, str) or not token:
        print(f"token exchange: refused (HTTP {status or 'no answer'})", file=out)
        return 1
    expires = body.get("expires_in")
    print("token exchange: accepted" + (f", expires in {expires} s" if isinstance(expires, int) else ""), file=out)
    print(f"environment: {environment}", file=out)
    print(f"project: {'set' if project else 'not set'}", file=out)
    apps, actions = catalog_entries(json.loads(Path(catalog_path).read_text()))
    print(f"catalog entries for pipedream: {len(apps)} apps, {len(actions)} actions", file=out)
    if not project:
        return 0
    api.token, api.environment = token, environment
    root = f"connect/{urllib.parse.quote(project)}/"
    problems = 0
    for app_id, slug in apps:
        status, found = api.send(root + "apps/" + urllib.parse.quote(slug))
        ok = status == 200 and isinstance(found, dict)
        state = "found" if ok else f"not found (HTTP {status or 'no answer'})"
        print(f"app {slug} ({app_id}): {state}", file=out)
        problems += not ok
    for action_id, component, fields in actions:
        status, found = api.send(root + "components/" + urllib.parse.quote(component))
        data = found.get("data", found) if isinstance(found, dict) else None
        if status != 200 or not isinstance(data, dict):
            print(f"component {component} ({action_id}): not found (HTTP {status or 'no answer'})", file=out)
            problems += 1
            continue
        result = compare(fields, data)
        print(f"component {component} ({action_id}): {describe(result)}", file=out)
        problems += bool(result["missing"] or result["unmapped_required"])
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(run(os.environ, sys.stdout))
