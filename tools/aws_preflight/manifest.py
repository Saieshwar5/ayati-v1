"""Validate a nonsecret, inactive M0 trial manifest before rendering a review."""
from datetime import datetime, timezone
from decimal import Decimal, InvalidOperation
import re


FIELDS = {
    "": {"schema_version", "phase", "account_id", "region", "environment", "scenario", "hostname",
         "hostname_status", "owner", "cost_center", "budget_usd", "expires_at", "release", "capacity",
         "database", "model", "inputs", "activation"},
    "scenario": {"employees", "agents", "active_test_minutes"},
    "release": {"source_commit", "image_uri", "schema_version"},
    "capacity": {"controller_type", "controller_count", "executor_count", "executor_max_count", "executor_cpu",
                 "executor_memory_mib", "slots_per_executor", "global_limit", "workspace_limit", "model_limit",
                 "chat_reserve", "run_seconds"},
    "database": {"engine_version", "class", "storage_gib"},
    "model": {"provider", "id"},
    "inputs": {"zone_a", "zone_b", "ami_id", "certificate_arn", "runtime_boundary_arn", "operator_role_arn",
               "runtime_database_secret_arn", "admin_secret_arn", "workos_secret_arn", "model_secret_arn", "workos_client_id"},
    "activation": {"controller_count", "executor_count", "autoscaling_enabled"},
}
GATES = [
    "M2: controller startup, restricted DB role/grants, verified RDS TLS, image/secret delivery and private storage proof",
    "M3: cross-host authenticated transport, exact executor/incarnation/slot binding and direct-backend denial",
    "M2/M3: Fargate scratch ownership, demand metrics, scale protection and empty-executor drain proof",
    "Reviewed numeric total budget and expiry; permission/quotas and price evidence refreshed immediately before deployment",
]


def validate(document: dict, now: datetime | None = None) -> dict:
    errors, missing = [], []
    if not isinstance(document, dict):
        return {"schema_ok": False, "errors": ["Manifest must be an object"], "missing": [], "deployment_ready": False}
    for section, fields in FIELDS.items():
        value = document if not section else document.get(section)
        if not isinstance(value, dict):
            errors.append(f"{section or 'manifest'} must be an object")
            continue
        for name in sorted(value.keys() - fields):
            # Do not echo unknown names/values: they could contain an accidental secret.
            errors.append(f"Unknown field in {section or 'manifest'}; secret values are not accepted")
        for name in sorted(fields - value.keys()):
            errors.append(f"Required field absent: {section + '.' if section else ''}{name}")
    if errors:
        return {"schema_ok": False, "errors": errors, "missing": missing, "deployment_ready": False}

    def value(path):
        current = document
        for part in path.split('.'):
            current = current[part]
        return current

    def pattern(path, regex):
        candidate = value(path)
        if candidate is None or candidate == "":
            missing.append(path)
            return
        if not isinstance(candidate, str) or re.fullmatch(regex, candidate) is None:
            errors.append(f"Invalid {path}")
        elif any(marker in candidate.lower() for marker in ("replace", "example", "changeme")):
            errors.append(f"Placeholder is not a deployable value: {path}")

    def exact(path, wanted):
        candidate = value(path)
        if type(candidate) is not type(wanted) or candidate != wanted:
            errors.append(f"Unsupported {path} for this bounded M0 profile")

    exact("schema_version", 1)
    exact("phase", "m0-preparation")
    exact("region", "ap-south-1")
    exact("scenario.employees", 5)
    exact("scenario.agents", 5)
    exact("scenario.active_test_minutes", 15)
    exact("release.schema_version", 17)
    exact("activation.controller_count", 0)
    exact("activation.executor_count", 0)
    exact("activation.autoscaling_enabled", False)
    pattern("account_id", r"[0-9]{12}")
    pattern("environment", r"ayati-[a-z0-9-]{1,18}")
    pattern("hostname", r"(?:[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.)*cuckoo\.onl")
    pattern("hostname_status", r"proposed-subdomain-of-owner-domain|confirmed")
    pattern("owner", r"[a-zA-Z0-9_-]{1,64}")
    pattern("cost_center", r"[a-zA-Z0-9_-]{1,64}")
    pattern("release.source_commit", r"[a-f0-9]{40}")
    pattern("database.engine_version", r"18\.[0-9]+")
    exact("database.class", "db.t4g.small")
    exact("database.storage_gib", 20)
    for field, wanted in {
        "controller_type": "t3.medium", "controller_count": 1,
        "executor_count": 2, "executor_max_count": 2, "executor_cpu": 2048,
        "executor_memory_mib": 4096, "slots_per_executor": 2,
        "global_limit": 4, "workspace_limit": 4, "model_limit": 4,
        "chat_reserve": 1, "run_seconds": 180,
    }.items():
        exact(f"capacity.{field}", wanted)
    pattern("model.provider", r"openai|fireworks-ai")
    pattern("model.id", r"[a-zA-Z0-9._:/-]{1,200}")
    account, env = document["account_id"], document["environment"]
    pattern("release.image_uri", rf"{re.escape(str(account))}\.dkr\.ecr\.ap-south-1\.amazonaws\.com/{re.escape(str(env))}/runtime@sha256:[a-f0-9]{{64}}")
    image = document["release"]["image_uri"]
    if isinstance(image, str) and image.endswith("0" * 64):
        errors.append("release.image_uri requires a real pushed registry manifest digest")
    for field in ("runtime_database_secret_arn", "admin_secret_arn", "workos_secret_arn", "model_secret_arn"):
        pattern("inputs." + field, rf"arn:aws:secretsmanager:ap-south-1:{re.escape(str(account))}:secret:[a-zA-Z0-9/_+=.@-]+")
    pattern("inputs.runtime_boundary_arn", rf"arn:aws:iam::{re.escape(str(account))}:policy/[a-zA-Z0-9/_+=,.@-]+")
    pattern("inputs.operator_role_arn", rf"arn:aws:iam::{re.escape(str(account))}:role/[a-zA-Z0-9/_+=,.@-]+")
    pattern("inputs.certificate_arn", rf"arn:aws:acm:ap-south-1:{re.escape(str(account))}:certificate/[a-f0-9-]{{36}}")
    pattern("inputs.ami_id", r"ami-[a-f0-9]{17}")
    for field in ("zone_a", "zone_b"):
        pattern("inputs." + field, r"ap-south-1[a-z]")
    pattern("inputs.workos_client_id", r"client_[a-zA-Z0-9]+")
    inputs = document["inputs"]
    if inputs["zone_a"] and inputs["zone_a"] == inputs["zone_b"]:
        errors.append("Two different availability zones are required")
    secrets = [inputs[name] for name in ("runtime_database_secret_arn", "admin_secret_arn", "workos_secret_arn", "model_secret_arn") if isinstance(inputs[name], str) and inputs[name]]
    if len(secrets) != len(set(secrets)):
        errors.append("Database admin/runtime, WorkOS and model credentials require distinct secret references")
    budget = document["budget_usd"]
    if budget is None:
        missing.append("budget_usd")
    else:
        try:
            amount = Decimal(budget) if isinstance(budget, str) else Decimal("NaN")
            if not amount.is_finite() or amount <= 0 or amount.as_tuple().exponent < -2:
                raise ValueError()
        except (InvalidOperation, ValueError):
            errors.append("budget_usd must be a positive finite USD string with at most two decimals")
    expiry = document["expires_at"]
    if expiry is None:
        missing.append("expires_at")
    else:
        try:
            instant = datetime.fromisoformat(expiry.replace("Z", "+00:00"))
            current = now or datetime.now(timezone.utc)
            if instant.tzinfo is None or instant.utcoffset().total_seconds() != 0:
                raise ValueError()
            if not 0 < (instant - current).total_seconds() <= 86400:
                raise ValueError()
        except (AttributeError, TypeError, ValueError):
            errors.append("expires_at must be a future UTC timestamp within 24 hours")
    return {"schema_ok": not errors, "errors": errors, "missing": sorted(missing),
            "configuration_complete": not errors and not missing,
            "deployment_ready": False, "activation_gates": GATES}
