"""Real local TLS, SDK signatures, streaming and disconnects; no AWS/model calls."""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import http.client
import json
from pathlib import Path
import sys
import tempfile
import threading
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent / "fixtures"))
from aws_gateway import HOST, start_chain
import urllib3

BOOTSTRAP = "c" * 43


@contextmanager
def backend():
    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"
        def log_message(self, *_):
            pass

        def do_POST(self):
            body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            self.server.requests.append((self.path, json.loads(body), dict(self.headers)))
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", "19")
            self.end_headers()
            self.wfile.write(b'{"registered":true}')

        def do_GET(self):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            try:
                self.wfile.write(b"data: first\n\n")
                self.wfile.flush()
                while not self.server.finish.wait(0.05):
                    self.wfile.write(b": keepalive\n\n")
                    self.wfile.flush()
            except OSError:
                self.server.disconnected.set()
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    server.requests = []
    server.finish = threading.Event()
    server.disconnected = threading.Event()
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        yield server
    finally:
        server.finish.set()
        server.shutdown()
        server.server_close()


class TransportTests(unittest.TestCase):
    def setUp(self):
        self.client = urllib3.PoolManager(timeout=urllib3.Timeout(connect=2, read=3), retries=False)
        self.addCleanup(self.client.clear)

    def test_scoped_tool_route_preserves_assignment_capability_through_signed_tls(self):
        assignment = "11111111-1111-4111-8111-111111111111"
        path = f"/v1/assignments/{assignment}/tools"
        capability = "s" * 43
        payload = {"assignment": {"scope": {"assignment": assignment}, "session": "test-session"},
                   "tool": "operations", "arguments": {}}
        with tempfile.TemporaryDirectory() as directory, backend() as origin:
            stack, chain = start_chain(Path(directory), f"http://127.0.0.1:{origin.server_port}", BOOTSTRAP)
            with stack:
                worker = chain["workers"][0]
                response = self.client.request("POST", worker["url"] + path,
                    body=json.dumps(payload), headers={"Authorization": "Bearer " + capability})
                self.assertEqual(response.status, 200)
                response.read()
                forwarded_path, forwarded, headers = origin.requests[0]
                self.assertEqual(forwarded_path, path)
                self.assertEqual(forwarded, payload)
                self.assertEqual(headers["Authorization"], "Bearer " + capability)
                self.assertNotEqual(headers["Authorization"], "Bearer " + BOOTSTRAP)
                self.assertTrue(headers["x-ayati-cloud-task"].endswith("a" * 32))
                response = self.client.request("GET", worker["url"] + path,
                    headers={"Authorization": "Bearer " + capability})
                self.assertEqual(response.status, 403)
                response.read()
                self.assertEqual(len(origin.requests), 1)

    def test_real_signed_tls_registration_overwrites_untrusted_identity(self):
        with tempfile.TemporaryDirectory() as directory, backend() as origin:
            stack, chain = start_chain(Path(directory), f"http://127.0.0.1:{origin.server_port}", BOOTSTRAP)
            with stack:
                worker = chain["workers"][0]
                body = {"executor": "one", "incarnation": "first", "token": "a" * 43, "pooled": True}
                response = self.client.request("POST", worker["url"] + "/v1/executors/register",
                    body=json.dumps(body), headers={"Authorization": "Bearer " + Path(worker["key_file"]).read_text(),
                    "x-ayati-verified-principal": "attacker-supplied", "x-ayati-cloud-task": "forged"})
                self.assertEqual(response.status, 200)
                path, forwarded, headers = origin.requests[0]
                self.assertEqual(path, "/v1/executors/register-cloud")
                self.assertEqual(forwarded["registration"], body)
                self.assertTrue(forwarded["task_arn"].endswith("a" * 32))
                self.assertTrue(forwarded["principal_arn"].endswith("a" * 32))
                self.assertEqual(headers["Authorization"], "Bearer " + BOOTSTRAP)
                self.assertNotIn("attacker", json.dumps(forwarded))

                # TLS trust and request authentication are separate boundaries.
                unsigned = urllib3.HTTPSConnectionPool("127.0.0.1", port=chain["gateway_port"],
                    ca_certs=str(Path(directory) / "gateway.crt"), assert_hostname=HOST,
                    server_hostname=HOST, retries=False)
                try:
                    self.assertEqual(unsigned.request("POST", "/trial/v1/executors/register", body=b"{}",
                        headers={"Host": HOST}).status, 403)
                finally:
                    unsigned.close()
                untrusted = urllib3.HTTPSConnectionPool("127.0.0.1", port=chain["gateway_port"],
                    assert_hostname=HOST, server_hostname=HOST, retries=False)
                try:
                    with self.assertRaises(urllib3.exceptions.SSLError):
                        untrusted.request("POST", "/trial/v1/executors/register", body=b"{}")
                finally:
                    untrusted.close()

    def test_stream_is_incremental_and_disconnect_reaches_upstream(self):
        with tempfile.TemporaryDirectory() as directory, backend() as origin:
            stack, chain = start_chain(Path(directory), f"http://127.0.0.1:{origin.server_port}", BOOTSTRAP)
            with stack:
                path = "/v1/executors/" + "a" * 36 + "/" + "b" * 36 + "/pool/events"
                response = self.client.request("GET", chain["workers"][0]["url"] + path,
                    headers={"Authorization": "Bearer " + "c" * 43}, preload_content=False)
                try:
                    self.assertEqual(response.status, 200)
                    self.assertEqual(response.read1(13), b"data: first\n\n")
                    self.assertFalse(origin.finish.is_set())
                finally:
                    response.close()
                    response.release_conn()
                self.assertTrue(origin.disconnected.wait(3), "Closed consumer retained the upstream stream")

    def test_wrong_method_duplicate_headers_and_browser_requests_are_denied(self):
        with tempfile.TemporaryDirectory() as directory, backend() as origin:
            stack, chain = start_chain(Path(directory), f"http://127.0.0.1:{origin.server_port}", BOOTSTRAP)
            with stack:
                worker = chain["workers"][0]
                for headers in ({"Origin": ""}, {"Cookie": ""}, {"Transfer-Encoding": "chunked"}):
                    response = self.client.request("POST", worker["url"] + "/v1/executors/register", body=b"{}", headers=headers)
                    self.assertEqual(response.status, 403)
                self.assertEqual(self.client.request("GET", worker["url"] + "/v1/executors/register").status, 403)
                port = int(worker["url"].rsplit(":", 1)[1])
                client = http.client.HTTPConnection("127.0.0.1", port, timeout=3)
                try:
                    client.putrequest("POST", "/v1/executors/register")
                    client.putheader("Authorization", "Bearer one")
                    client.putheader("Authorization", "Bearer two")
                    client.putheader("Content-Length", "0")
                    client.endheaders()
                    self.assertEqual(client.getresponse().status, 403)
                finally:
                    client.close()
                self.assertEqual(origin.requests, [])

    def test_model_credentials_stay_at_controller_and_destinations_are_local(self):
        with tempfile.TemporaryDirectory() as directory, backend() as origin:
            root = Path(directory)
            model = root / "model.json"
            model.write_text(json.dumps({"url": f"http://127.0.0.1:{origin.server_port}", "key": "u" * 43}))
            model.chmod(0o600)
            stack, chain = start_chain(root, f"http://127.0.0.1:{origin.server_port}", BOOTSTRAP, model)
            with stack:
                worker = chain["workers"][0]
                key = Path(worker["model_key_file"]).read_text()
                response = self.client.request("POST", worker["model_url"] + "/v1/chat/completions",
                    body=b'{"model":"fixture"}', headers={"Authorization": "Bearer " + key})
                self.assertEqual(response.status, 200)
                self.assertEqual(origin.requests[-1][0], "/v1/chat/completions")
                self.assertEqual(origin.requests[-1][2]["Authorization"], "Bearer " + "u" * 43)
                denied = self.client.request("POST", worker["model_url"] + "/v1/chat/completions",
                    body=b"{}", headers={"Authorization": "Bearer " + "u" * 43})
                self.assertEqual(denied.status, 403)
                model.write_text(json.dumps({"url": "https://outside.invalid", "key": "u" * 43}))
                refused = self.client.request("POST", worker["model_url"] + "/v1/chat/completions",
                    body=b"{}", headers={"Authorization": "Bearer " + key})
                self.assertEqual(refused.status, 400)
                self.assertNotIn(("u" * 43).encode(), refused.data)


if __name__ == "__main__":
    unittest.main()
