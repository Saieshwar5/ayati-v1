"""Explicit loopback bridge startup; infrastructure admission remains a separate gate."""
from contextlib import ExitStack
from pathlib import Path
import os
import re
import stat

from .config import TOKEN, cloud_identity, gateway_url, local_origin, private_json, private_read, private_token
from .server import start_server
from .transport import TaskVerifier, Transport


WORKER_FIELDS = {"mode", "gateway", "control_port", "model_port", "control_key_file", "model_key_file"}
CONTROLLER_FIELDS = {"mode", "listen_port", "control_url", "bootstrap_file", "model_info_file",
                     "role_arn", "cluster_arn", "release_arn"}
CORE_SECRETS = ("RELAY_DATABASE_URL", "RELAY_SIMULATION_DATABASE_URL", "WORKOS_API_KEY", "WORK_OS_API_KEY",
                "OPENAI_API_KEY", "FIREWORKS_API_KEY")


def private_path(value):
    if not isinstance(value, str) or not Path(value).is_absolute():
        raise ValueError("Bridge files require absolute paths")
    return Path(value)


def port(value):
    if type(value) is not int or not 1024 <= value <= 65535:
        raise ValueError("Bridge ports must be explicit unprivileged TCP ports")
    return value


def validate(document):
    if not isinstance(document, dict):
        raise ValueError("Bridge configuration must be an object")
    mode = document.get("mode")
    fields = WORKER_FIELDS if mode == "worker" else CONTROLLER_FIELDS if mode == "controller" else None
    if fields is None or document.keys() != fields:
        raise ValueError("Unknown or missing bridge configuration fields")
    if mode == "worker":
        gateway_url(document["gateway"])
        ports = [port(document[name]) for name in ("control_port", "model_port")]
        paths = [private_path(document[name]) for name in ("control_key_file", "model_key_file")]
        if ports[0] == ports[1] or paths[0] == paths[1]:
            raise ValueError("Control and model endpoints require distinct ports and files")
    else:
        port(document["listen_port"])
        local_origin(document["control_url"])
        for name in ("bootstrap_file", "model_info_file"):
            private_path(document[name])
        role, cluster, release = (document[name] for name in ("role_arn", "cluster_arn", "release_arn"))
        if not all(isinstance(value, str) for value in (role, cluster, release)):
            raise ValueError("Explicit AWS infrastructure identities required")
        match = re.fullmatch(r"arn:aws:iam::([0-9]{12}):role/(ayati-[A-Za-z0-9-]+)", role)
        if not match:
            raise ValueError("Invalid executor role")
        cloud_identity(f"arn:aws:sts::{match[1]}:assumed-role/{match[2]}/" + "a" * 32, role, cluster)
        if not re.fullmatch(rf"arn:aws:ecs:ap-south-1:{match[1]}:task-definition/ayati-[A-Za-z0-9-]+:[1-9][0-9]*", release):
            raise ValueError("Wrong executor release")
    return document


def local_key(path):
    # Creation is permitted only within an existing private, owner-owned directory.
    # A task must not reuse the controller bootstrap or another task's scratch.
    parent = path.parent
    info = parent.lstat()
    if (not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid()
            or info.st_mode & 0o777 != 0o700 or parent.resolve() != parent):
        raise ValueError("Task keys require an owner-owned private directory without symlinks")
    return private_token(path)


class BridgeRuntime:
    def __init__(self, document, *, transport=None, verifier=None):
        self.config = validate(document)
        self.transport = transport
        self.verifier = verifier
        self.stack = ExitStack()
        self.endpoints = {}

    def __enter__(self):
        try:
            config = self.config
            transport = self.transport or Transport(config.get("gateway"))
            clear = getattr(transport.http, "clear", None)
            if clear:
                self.stack.callback(clear)
            if config["mode"] == "worker":
                keys = [local_key(Path(config[name])) for name in ("control_key_file", "model_key_file")]
                if keys[0] == keys[1]:
                    raise ValueError("Control and model credentials must differ")
                for index, kind in enumerate(("control", "model")):
                    self.listen(kind, config[kind + "_port"], mode="worker", model=kind == "model",
                                local_key=keys[index], transport=transport)
            else:
                key = private_read(Path(config["bootstrap_file"]), 44).rstrip("\n")
                model = private_json(Path(config["model_info_file"]))
                if set(model) != {"url", "key"} or not TOKEN.fullmatch(model.get("key", "")) or model["key"] == key:
                    raise ValueError("Separate private model capability required")
                local_origin(model["url"])
                if not TOKEN.fullmatch(key):
                    raise ValueError("Invalid controller bootstrap")
                verifier = self.verifier or TaskVerifier(config["role_arn"], config["cluster_arn"], config["release_arn"])
                if (verifier.role, verifier.cluster, verifier.release) != tuple(config[name] for name in ("role_arn", "cluster_arn", "release_arn")):
                    raise ValueError("Verifier identity differs from configured release")
                self.listen("controller", config["listen_port"], mode="controller", verifier=verifier,
                            local_key=key, transport=transport, control_url=config["control_url"],
                            model_info=Path(config["model_info_file"]))
            return self
        except BaseException:
            self.stack.close()
            raise

    def listen(self, name, number, **settings):
        server = start_server(("127.0.0.1", number), **settings)
        self.stack.callback(server.server_close)
        self.stack.callback(server.shutdown)
        self.endpoints[name] = f"http://127.0.0.1:{server.server_port}"

    def __exit__(self, *error):
        return self.stack.__exit__(*error)


def reject_core_secrets(environment):
    if any(environment.get(name) or environment.get(name + "_FILE") for name in CORE_SECRETS):
        raise ValueError("Bridge processes must not receive database, WorkOS or provider credentials")
