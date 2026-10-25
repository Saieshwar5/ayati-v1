"""Strict infrastructure identities; caller-supplied task metadata is not authentication."""
import json
import os
from pathlib import Path
import re
import secrets
import stat
from urllib.parse import urlsplit


CONTROL = re.compile(r"/v1/(?:executors/[a-f0-9-]{36}/[a-f0-9-]{36}/(?:pool/(?:events|capacity|drain|stop)|release)|assignments/[a-f0-9-]{36}/(?:start|renew|complete|output|tools))")
TOKEN = re.compile(r"[A-Za-z0-9_-]{43}")


def route_allowed(method: str, path: str) -> bool:
    if path in ("/v1/executors/register", "/v1/trial-model/chat/completions", "/v1/chat/completions"):
        return method == "POST"
    if path in ("/v1/trial-model/models", "/v1/models"):
        return method == "GET"
    return bool(CONTROL.fullmatch(path)) and method == ("GET" if path.endswith("/pool/events") else "POST")


def local_origin(value: str) -> str:
    parsed = urlsplit(value)
    if (parsed.scheme != "http" or parsed.hostname != "127.0.0.1" or not parsed.port
            or parsed.username is not None or parsed.password is not None
            or parsed.path not in ("", "/") or parsed.query or parsed.fragment):
        raise ValueError("Use an explicit IPv4 loopback HTTP origin")
    return f"http://127.0.0.1:{parsed.port}"


def cloud_identity(principal: str, role_arn: str, cluster_arn: str) -> tuple[str, str]:
    role = re.fullmatch(r"arn:aws:iam::([0-9]{12}):role/(ayati-[A-Za-z0-9-]+)", role_arn)
    cluster = re.fullmatch(r"arn:aws:ecs:ap-south-1:([0-9]{12}):cluster/(ayati-[A-Za-z0-9-]+)", cluster_arn)
    if not role or not cluster or role[1] != cluster[1]:
        raise ValueError("Invalid configured cloud identity")
    match = re.fullmatch(rf"arn:aws:sts::{role[1]}:assumed-role/{re.escape(role[2])}/([a-f0-9]{{32}})", principal)
    if not match:
        raise PermissionError("Wrong IAM task principal")
    return f"arn:aws:ecs:ap-south-1:{role[1]}:task/{cluster[2]}/{match[1]}", match[1]


def gateway_url(value: str) -> str:
    if not re.fullmatch(r"https://[a-z0-9]+\.execute-api\.ap-south-1\.amazonaws\.com/trial", value):
        raise ValueError("Use the exact regional HTTPS trial gateway")
    return value


def private_token(path: Path) -> str:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    try:
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    except FileExistsError:
        descriptor = None
    if descriptor is not None:
        with os.fdopen(descriptor, "w") as output:
            output.write(secrets.token_urlsafe(32))
    value = private_read(path, 44).rstrip("\n")
    if not TOKEN.fullmatch(value):
        raise ValueError("Invalid private transport token")
    return value


def private_json(path: Path) -> dict:
    value = json.loads(private_read(path, 16384))
    if not isinstance(value, dict):
        raise ValueError("Private bridge configuration must be an object")
    return value


def private_read(path: Path, limit: int) -> str:
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "r") as file:
        info = os.fstat(file.fileno())
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
                or info.st_mode & 0o777 != 0o600 or info.st_size > limit):
            raise ValueError("Private owner-owned regular file required (0600)")
        value = file.read(limit + 1)
        if len(value) > limit:
            raise ValueError("Private bridge file exceeds its limit")
        return value
