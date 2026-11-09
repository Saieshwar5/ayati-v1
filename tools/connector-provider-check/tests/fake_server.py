"""A local HTTP stand-in for provider APIs: routes map (method, path) to a
(status, JSON) answer or a function of the request; requests are recorded."""
import importlib.util
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1]


def load(name):
    """Imports a check script by file name, the way it runs: standalone."""
    spec = importlib.util.spec_from_file_location(name, TOOLS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class FakeServer:
    def __init__(self, routes):
        self.routes, self.requests = routes, []
        fake = self

        class Handler(BaseHTTPRequestHandler):
            def answer(self, method):
                length = int(self.headers.get("content-length") or 0)
                body = json.loads(self.rfile.read(length)) if length else None
                fake.requests.append({"method": method, "path": self.path, "headers": {k.lower(): v for k, v in self.headers.items()}, "body": body})
                route = fake.routes.get((method, self.path))
                status, data = route(self.headers, body) if callable(route) else route or (404, {"error": "not found"})
                payload = json.dumps(data).encode()
                self.send_response(status)
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

            def do_GET(self):
                self.answer("GET")

            def do_POST(self):
                self.answer("POST")

            def log_message(self, *args):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *args):
        self.server.shutdown()
        self.server.server_close()

    def base(self, prefix):
        return f"http://127.0.0.1:{self.server.server_address[1]}{prefix}"
