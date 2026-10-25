"""Loopback TLS/SigV4 rehearsal. Synthetic credentials are never sent to AWS.

The gateway replaces the principal header after checking a real SDK signature.
This models the intended IAM integration; it does not prove AWS authorizes it.
"""
from contextlib import ExitStack
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import ssl
import subprocess
import sys
import threading
from urllib.parse import urlsplit

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "integrations"))
import boto3
from botocore.auth import SigV4Auth
from botocore.awsrequest import AWSRequest
from botocore.credentials import Credentials
import urllib3

from aws_bridge.config import cloud_identity
from aws_bridge.startup import BridgeRuntime
from aws_bridge.transport import Transport

HOST = "local.execute-api.ap-south-1.amazonaws.com"
ROLE = "arn:aws:iam::123456789012:role/ayati-test-executor"
CLUSTER = "arn:aws:ecs:ap-south-1:123456789012:cluster/ayati-test"
RELEASE = "arn:aws:ecs:ap-south-1:123456789012:task-definition/ayati-test:1"


def principal(task):
    return f"arn:aws:sts::123456789012:assumed-role/ayati-test-executor/{task}"


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


class FixtureVerifier:
    role, cluster, release = ROLE, CLUSTER, RELEASE

    def verify(self, identity):
        task, identifier = cloud_identity(identity, self.role, self.cluster)
        if identifier not in ("a" * 32, "b" * 32):
            raise PermissionError()
        return task


class GatewayHandler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        self.forward()

    def do_POST(self):
        self.forward()

    def forward(self):
        response = None
        sent = False
        try:
            size = int(self.headers.get("Content-Length", "0"))
            if not 0 <= size <= 524288:
                raise PermissionError()
            body = self.rfile.read(size)
            identity = self.verify_signature(body)
            if not self.path.startswith("/trial/v1/"):
                raise PermissionError()
            response = self.server.transport.request(self.command,
                self.server.controller + self.path.removeprefix("/trial"), body,
                {"x-ayati-verified-principal": identity,
                 "x-ayati-capability": self.headers.get("x-ayati-capability", "")})
            self.send_response(response.status)
            self.send_header("Content-Type", response.headers.get("Content-Type", "application/json"))
            self.send_header("Connection", "close")
            self.end_headers()
            sent = True
            while chunk := response.read1(16384):
                self.wfile.write(chunk)
                self.wfile.flush()
        except Exception:
            if not sent:
                self.send_response(403)
                self.send_header("Content-Length", "0")
                self.end_headers()
        finally:
            if response is not None:
                response.close()
                response.release_conn()
            self.close_connection = True

    def verify_signature(self, body):
        authorization = self.headers.get("Authorization", "")
        prefix = "AWS4-HMAC-SHA256 Credential="
        if not authorization.startswith(prefix):
            raise PermissionError()
        credential, signed, signature = authorization[len(prefix):].split(", ")
        access, day, region, service, end = credential.split("/")
        if (region, service, end) != ("ap-south-1", "execute-api", "aws4_request"):
            raise PermissionError()
        secret, identity = self.server.identities[access]
        names = signed.removeprefix("SignedHeaders=").split(";")
        if not {"host", "x-amz-date", "x-ayati-capability"} <= set(names):
            raise PermissionError()
        if self.headers["Host"] != HOST or any(len(self.headers.get_all(n, [])) != 1 for n in names):
            raise PermissionError()
        request = AWSRequest(method=self.command, url=f"https://{HOST}{self.path}",
                             data=body, headers={name: self.headers[name] for name in names})
        timestamp = self.headers["X-Amz-Date"]
        if not timestamp.startswith(day):
            raise PermissionError()
        request.context["timestamp"] = timestamp
        signer = SigV4Auth(Credentials(access, secret), service, region)
        expected = signer.signature(signer.string_to_sign(request, signer.canonical_request(request)), request)
        if not secrets.compare_digest(signature, "Signature=" + expected):
            raise PermissionError()
        return identity


class LoopbackTLS:
    """Connect the fixed test hostname only to this fixture's loopback TLS socket."""
    def __init__(self, gateway, certificate):
        self.pool = urllib3.HTTPSConnectionPool("127.0.0.1", port=gateway.server_port,
            cert_reqs="CERT_REQUIRED", ca_certs=str(certificate),
            assert_hostname=HOST, server_hostname=HOST,
            timeout=urllib3.Timeout(connect=2, read=10), maxsize=16)

    def request(self, method, url, **kwargs):
        parsed = urlsplit(url)
        if parsed.scheme != "https" or parsed.netloc != HOST or parsed.query:
            raise ValueError("Fixture only connects its own TLS gateway")
        kwargs["headers"] = {**kwargs.get("headers", {}), "Host": HOST}
        return self.pool.request(method, parsed.path, **kwargs)


def start_chain(directory, control_url, bootstrap, model_info=None):
    """Start real HTTP bridges and a signed TLS front door; return cleanup and settings."""
    stack = ExitStack()
    try:
        bootstrap_file = directory / "controller.key"
        bootstrap_file.write_text(bootstrap)
        bootstrap_file.chmod(0o600)
        if not model_info:
            model_info = directory / "unused-model.json"
            model_info.write_text(json.dumps({"url": "http://127.0.0.1:9", "key": secrets.token_urlsafe(32)}))
            model_info.chmod(0o600)
        controller = stack.enter_context(BridgeRuntime({"mode": "controller", "listen_port": free_port(),
            "bootstrap_file": str(bootstrap_file), "control_url": control_url, "model_info_file": str(model_info),
            "role_arn": ROLE, "cluster_arn": CLUSTER, "release_arn": RELEASE}, verifier=FixtureVerifier()))
        gateway = ThreadingHTTPServer(("127.0.0.1", 0), GatewayHandler)
        gateway.daemon_threads = True
        gateway.transport = Transport()
        gateway.controller = controller.endpoints["controller"]
        gateway.identities = {}
        certificate, key = directory / "gateway.crt", directory / "gateway.key"
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
            "-subj", f"/CN={HOST}", "-addext", f"subjectAltName=DNS:{HOST}",
            "-keyout", str(key), "-out", str(certificate)], check=True,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(certificate, key)
        gateway.socket = context.wrap_socket(gateway.socket, server_side=True)
        threading.Thread(target=gateway.serve_forever, daemon=True).start()
        stack.callback(gateway.server_close)
        stack.callback(gateway.shutdown)
        workers = []
        for number, task in enumerate(("a" * 32, "b" * 32)):
            access, secret = f"LOCALFIXTURE{number}", secrets.token_hex(24)
            gateway.identities[access] = (secret, principal(task))
            transport = Transport(f"https://{HOST}/trial", session=boto3.Session(
                aws_access_key_id=access, aws_secret_access_key=secret, region_name="ap-south-1"))
            transport.http = LoopbackTLS(gateway, certificate)
            stack.callback(transport.http.pool.close)
            keyfile = directory / f"worker-{number}.key"
            model_key = directory / f"model-{number}.key"
            worker = stack.enter_context(BridgeRuntime({"mode": "worker", "gateway": transport.gateway,
                "control_port": free_port(), "model_port": free_port(),
                "control_key_file": str(keyfile), "model_key_file": str(model_key)}, transport=transport))
            workers.append({"url": worker.endpoints["control"], "key_file": str(keyfile), "task": task,
                            "model_url": worker.endpoints["model"], "model_key_file": str(model_key)})
        return stack, {"controller": gateway.controller, "gateway_port": gateway.server_port,
                       "workers": workers}
    except BaseException:
        stack.close()
        raise


def main():
    config = json.loads(Path(sys.argv[1]).read_text())
    directory = Path(config["directory"])
    stopped = threading.Event()
    for event in (signal.SIGINT, signal.SIGTERM):
        signal.signal(event, lambda *_: stopped.set())
    stack, settings = start_chain(directory, config["control_url"],
        Path(config["bootstrap_file"]).read_text().strip(),
        Path(config["model_info"]) if config.get("model_info") else None)
    with stack:
        descriptor = os.open(directory / "bridge-ready.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w") as file:
            json.dump(settings, file)
        print("BRIDGE_READY " + json.dumps(settings), flush=True)
        stopped.wait()


if __name__ == "__main__":
    main()
