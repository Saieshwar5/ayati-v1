"""Local signing proxy and private IAM front-door adapter for the Rust machine API."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import logging
import re
import secrets
import socket
import threading

from .config import CONTROL, TOKEN, cloud_identity, local_origin, private_json, route_allowed


class BridgeServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def __init__(self, *args, **kwargs):
        self.capacity = threading.BoundedSemaphore(32)
        self.stopping = threading.Event()
        self.connections = set()
        self.connection_lock = threading.Lock()
        super().__init__(*args, **kwargs)

    def process_request(self, request, address):
        if not self.capacity.acquire(blocking=False):
            request.close()
            return
        try:
            with self.connection_lock:
                self.connections.add(request)
            super().process_request(request, address)
        except Exception:
            with self.connection_lock:
                self.connections.discard(request)
            self.capacity.release()
            raise

    def process_request_thread(self, *args):
        try:
            super().process_request_thread(*args)
        finally:
            with self.connection_lock:
                self.connections.discard(args[0])
            self.capacity.release()

    def server_close(self):
        self.stopping.set()
        with self.connection_lock:
            for connection in self.connections:
                try:
                    connection.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
        super().server_close()


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def setup(self):
        super().setup()
        self.connection.settimeout(10)

    def log_message(self, *_):
        pass  # Default HTTP logs include request-controlled strings.

    def do_GET(self):
        self.handle_request("GET")

    def do_POST(self):
        self.handle_request("POST")

    def fail(self, code):
        payload = json.dumps({"error": "Cloud bridge request denied"}).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(payload)

    def single_header(self, name):
        values = self.headers.get_all(name, [])
        if len(values) > 1:
            raise PermissionError()
        return values[0] if values else ""

    def handle_request(self, method):
        sent = False
        complete = False
        response = None
        try:
            if self.path == "/health" and method == "GET":
                self.send_response(204)
                self.send_header("Content-Length", "0")
                self.send_header("Connection", "close")
                self.end_headers()
                return
            if (not route_allowed(method, self.path) or any(name in self.headers for name in
                    ("Origin", "Cookie", "Transfer-Encoding"))):
                raise PermissionError()
            for name in ("Authorization", "x-ayati-capability", "x-ayati-verified-principal"):
                self.single_header(name)
            lengths = self.headers.get_all("Content-Length", [])
            if len(lengths) > 1:
                raise PermissionError()
            length = int(lengths[0]) if lengths else 0
            if not 0 <= length <= 524288:
                raise PermissionError()
            if method == "GET" and length:
                raise PermissionError()
            body = self.rfile.read(length) if length else b""
            if len(body) != length:
                raise PermissionError()
            if self.server.mode == "worker":
                response = self.worker(method, body)
            else:
                response = self.controller(method, body)
            self.send_response(response.status)
            for name in ("Content-Type", "Cache-Control"):
                if response.headers.get(name):
                    self.send_header(name, response.headers[name])
            self.send_header("Transfer-Encoding", "chunked")
            self.send_header("Connection", "close")
            self.end_headers()
            sent = True
            while not self.server.stopping.is_set():
                chunk = response.read1(16384)
                if not chunk:
                    complete = True
                    break
                self.wfile.write(f"{len(chunk):x}\r\n".encode() + chunk + b"\r\n")
                self.wfile.flush()
            if complete:
                self.wfile.write(b"0\r\n\r\n")
        except PermissionError:
            if not sent:
                self.fail(403)
        except (BrokenPipeError, ConnectionResetError):
            pass
        except (ValueError, TypeError):
            if not sent:
                self.fail(400)
        except Exception as error:
            logging.getLogger(__name__).warning("Cloud bridge %s failed (%s)",
                "stream" if sent else "request", type(error).__name__)
            if not sent:
                self.fail(503)
        finally:
            if response is not None:
                if not complete:
                    response.close()
                response.release_conn()
            self.close_connection = True

    def worker(self, method, body):
        if self.client_address[0] != "127.0.0.1":
            raise PermissionError()
        authorization = self.headers.get("Authorization", "")
        if self.server.model:
            if not secrets.compare_digest(authorization, "Bearer " + self.server.local_key):
                raise PermissionError()
            routes = {"/v1/chat/completions": "/v1/trial-model/chat/completions", "/v1/models": "/v1/trial-model/models"}
            if self.path not in routes:
                raise PermissionError()
            path, token = routes[self.path], "model"
        else:
            path = self.path
            if path == "/v1/executors/register":
                if not secrets.compare_digest(authorization, "Bearer " + self.server.local_key):
                    raise PermissionError()
                token = "register"
            elif CONTROL.fullmatch(path) and authorization.startswith("Bearer ") and TOKEN.fullmatch(authorization[7:]):
                token = authorization[7:]
            else:
                raise PermissionError()
        return self.server.transport.request(method, self.server.transport.gateway + path, body,
                                              {"x-ayati-capability": token}, signed=True)

    def controller(self, method, body):
        principals = self.headers.get_all("x-ayati-verified-principal", [])
        if len(principals) != 1:
            raise PermissionError()
        principal = principals[0]
        task, _ = cloud_identity(principal, self.server.verifier.role, self.server.verifier.cluster)
        token = self.headers.get("x-ayati-capability", "")
        headers = {"x-ayati-cloud-task": task}
        if self.path == "/v1/executors/register" and method == "POST":
            # Derive infrastructure identity from AWS's overwritten integration header,
            # then verify the actual ECS task. Never forward body-supplied metadata.
            self.server.verifier.verify(principal)
            registration = json.loads(body)
            if set(registration) != {"executor", "incarnation", "token", "pooled"}:
                raise PermissionError()
            body = json.dumps({"registration": registration, "task_arn": task,
                               "principal_arn": principal, "release_arn": self.server.verifier.release}).encode()
            path = "/v1/executors/register-cloud"
            headers["Authorization"] = "Bearer " + self.server.local_key
        elif self.path.startswith("/v1/trial-model/"):
            if self.path not in ("/v1/trial-model/models", "/v1/trial-model/chat/completions"):
                raise PermissionError()
            self.server.verifier.verify(principal)
            model = private_json(self.server.model_info)
            if not TOKEN.fullmatch(model.get("key", "")):
                raise ValueError("Invalid local model capability")
            suffix = "/v1/models" if self.path.endswith("/models") else "/v1/chat/completions"
            return self.server.transport.request(method, local_origin(model["url"]) + suffix, body,
                                                  {"Authorization": "Bearer " + model["key"]})
        elif CONTROL.fullmatch(self.path) and TOKEN.fullmatch(token):
            path = self.path
            headers["Authorization"] = "Bearer " + token
        else:
            raise PermissionError()
        return self.server.transport.request(method, self.server.control_url + path, body, headers)


def start_server(address, **settings):
    if settings.get("mode") not in ("worker", "controller"):
        raise ValueError("Choose worker or controller bridge mode")
    settings["control_url"] = local_origin(settings.get("control_url", "http://127.0.0.1:8766"))
    server = BridgeServer(address, Handler)
    for name, value in settings.items():
        setattr(server, name, value)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()
    return server
