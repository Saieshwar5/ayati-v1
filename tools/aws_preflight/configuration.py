"""Reviewable stack inputs and role settings, without credentials or deployment calls."""
from .templates import ORDER


def stack_parameters(manifest: dict) -> dict:
    inputs, release = manifest["inputs"], manifest["release"]
    values = {
        "network": {"ZoneA": inputs["zone_a"], "ZoneB": inputs["zone_b"]},
        "data": {"PostgresVersion": manifest["database"]["engine_version"]},
        "operations": {},
        "permissions": {"RuntimeBoundaryArn": inputs["runtime_boundary_arn"], "OperatorRoleArn": inputs["operator_role_arn"],
                        "RuntimeDatabaseSecretArn": inputs["runtime_database_secret_arn"], "WorkosSecretArn": inputs["workos_secret_arn"],
                        "ModelSecretArn": inputs["model_secret_arn"], "AdminSecretArn": inputs["admin_secret_arn"]},
        "controller": {"AmiId": inputs["ami_id"]},
        "executors": {"ImageUri": release["image_uri"], "Model": manifest["model"]["id"],
                      "ModelProvider": manifest["model"]["provider"], "ModelSecretArn": inputs["model_secret_arn"]},
        "edge": {"Hostname": manifest["hostname"], "CertificateArn": inputs["certificate_arn"]},
    }
    return {name: {"ExpectedAccountId": manifest["account_id"], "Environment": manifest["environment"], **values[name]} for name in ORDER}


def controller_environment(manifest: dict) -> dict:
    """An M2 integration contract, deliberately not a runnable cloud bootstrap."""
    caps = manifest["capacity"]
    return {
        "AYATI_PUBLIC_ORIGIN": "https://" + manifest["hostname"] if manifest["hostname"] else None,
        "AYATI_BIND_ADDRESS": "0.0.0.0", "AYATI_CHAT_EXECUTION": "pooled",
        "AYATI_WORKER_PROVIDERS": manifest["model"]["provider"],
        "AYATI_MODEL": manifest["model"]["id"], "AWS_REGION": manifest["region"],
        "AYATI_EXECUTOR_CAPACITY": str(caps["global_limit"]),
        "AYATI_EXECUTOR_WORKSPACE_LIMIT": str(caps["workspace_limit"]),
        "AYATI_EXECUTOR_MODEL_LIMIT": str(caps["model_limit"]),
        "AYATI_EXECUTOR_CHAT_RESERVE": str(caps["chat_reserve"]),
        "AYATI_ASSIGNMENT_RUN_SECONDS": str(caps["run_seconds"]),
        "RELAY_DATABASE_URL_FILE": "/run/ayati-secrets/database_url",
        "WORKOS_API_KEY_FILE": "/run/ayati-secrets/workos_api_key",
        "WORKOS_CLIENT_ID": manifest["inputs"]["workos_client_id"],
        "CUCKOO_LIBRARY_S3_BUCKET": f"{manifest['environment']}-{manifest['account_id']}-{manifest['region']}-files" if manifest["account_id"] else None,
    }


def stack_tags(manifest: dict) -> dict:
    return {"application": "ayati", "environment": manifest["environment"], "owner": manifest["owner"],
            "managed-by": "cloudformation", "cost-center": manifest["cost_center"],
            "expires-at": manifest["expires_at"], "release": manifest["release"]["source_commit"]}
