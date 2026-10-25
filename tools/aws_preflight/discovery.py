"""Bounded, explicitly allowlisted AWS CLI reads. Never retrieve secret values."""
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import subprocess


READS = {
    "identity": ["sts", "get-caller-identity"],
    "vpcs": ["ec2", "describe-vpcs", "--query", "Vpcs[].{id:VpcId,default:IsDefault,cidr:CidrBlock}"],
    "instances": ["ec2", "describe-instances", "--query", "Reservations[].Instances[].{id:InstanceId,type:InstanceType,state:State.Name}"],
    "nat_gateways": ["ec2", "describe-nat-gateways", "--query", "NatGateways[].{id:NatGatewayId,state:State}"],
    "addresses": ["ec2", "describe-addresses", "--query", "Addresses[].AllocationId"],
    "rds": ["rds", "describe-db-instances", "--query", "DBInstances[].{id:DBInstanceIdentifier,class:DBInstanceClass,engine:Engine,version:EngineVersion,status:DBInstanceStatus}"],
    "clusters": ["ecs", "list-clusters"],
    "repositories": ["ecr", "describe-repositories", "--query", "repositories[].{name:repositoryName,arn:repositoryArn}"],
    "load_balancers": ["elbv2", "describe-load-balancers", "--query", "LoadBalancers[].{arn:LoadBalancerArn,scheme:Scheme,state:State.Code}"],
    "stacks": ["cloudformation", "describe-stacks", "--query", "Stacks[].{name:StackName,status:StackStatus}"],
    "certificates": ["acm", "list-certificates", "--query", "CertificateSummaryList[].{arn:CertificateArn,status:Status}"],
    "secret_metadata": ["secretsmanager", "list-secrets", "--query", "SecretList[].ARN"],
    "log_groups": ["logs", "describe-log-groups", "--query", "logGroups[].logGroupName"],
    "buckets": ["s3api", "list-buckets", "--query", "Buckets[].Name"],
    "zones": ["route53", "list-hosted-zones", "--query", "HostedZones[].{id:Id,private:Config.PrivateZone}"],
    "postgres_versions": ["rds", "describe-db-engine-versions", "--engine", "postgres", "--query", "DBEngineVersions[].{version:EngineVersion,status:Status,family:DBParameterGroupFamily}"],
    "postgres_orderability": ["rds", "describe-orderable-db-instance-options", "--engine", "postgres", "--db-instance-class", "db.t4g.small", "--query", "OrderableDBInstanceOptions[].{version:EngineVersion,storage:StorageType,multi_az:MultiAZCapable}"],
    "controller_types": ["ec2", "describe-instance-type-offerings", "--location-type", "region", "--filters", "Name=instance-type,Values=t3.medium", "--query", "InstanceTypeOfferings[].InstanceType"],
    "availability_zones": ["ec2", "describe-availability-zones", "--filters", "Name=state,Values=available", "--query", "AvailabilityZones[].{name:ZoneName,id:ZoneId,state:State}"],
    "controller_ami": ["ssm", "get-parameter", "--name", "/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-x86_64", "--query", "Parameter.{name:Name,value:Value,version:Version}"],
    "controller_quota": ["service-quotas", "get-service-quota", "--service-code", "ec2", "--quota-code", "L-1216C47A", "--query", "Quota.{name:QuotaName,value:Value,code:QuotaCode}"],
}
for _service in ("fargate", "rds", "vpc"):
    READS[f"quotas_{_service}"] = [
        "service-quotas", "list-service-quotas", "--service-code", _service,
        "--query", "Quotas[].{name:QuotaName,code:QuotaCode,value:Value,adjustable:Adjustable}",
    ]
READ_OPERATIONS = {tuple(command[:2]) for command in READS.values()} | {
    ("iam", "get-role"), ("iam", "list-attached-role-policies"), ("iam", "list-role-policies"),
    ("pricing", "get-products"), ("cloudformation", "validate-template"), ("ec2", "describe-images"),
}


def aws_read(arguments: list[str], region: str, timeout: int = 25) -> dict:
    """Run a known read; callers must never pass a user-supplied AWS operation."""
    if tuple(arguments[:2]) not in READ_OPERATIONS:
        raise ValueError("AWS operation is not an allowlisted preflight read")
    if arguments[:2] == ["ssm", "get-parameter"] and arguments != READS["controller_ami"]:
        raise ValueError("Only the public AWS AMI metadata parameter is allowed")
    env = {**os.environ, "AWS_PAGER": "", "AWS_CLI_AUTO_PROMPT": "off", "AWS_MAX_ATTEMPTS": "1"}
    command = ["aws", *arguments, "--region", region, "--output", "json",
               "--no-cli-pager", "--cli-connect-timeout", "5", "--cli-read-timeout", "15"]
    try:
        result = subprocess.run(command, capture_output=True, text=True, env=env, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"status": "blocked", "reason": type(error).__name__}
    if result.returncode:
        code = re.search(r"An error occurred \(([^)]+)\)", result.stderr)
        return {"status": "blocked", "reason": code[1] if code else "CLIError",
                "exit_code": result.returncode}
    try:
        return {"status": "pass", "data": json.loads(result.stdout)}
    except json.JSONDecodeError:
        return {"status": "blocked", "reason": "InvalidCLIJSON"}


def discover(region: str) -> dict:
    with ThreadPoolExecutor(max_workers=4) as pool:
        values = list(pool.map(lambda item: (item[0], aws_read(item[1], region)), READS.items()))
    checks = dict(values)
    ami = checks["controller_ami"].get("data", {}).get("value", "")
    if isinstance(ami, str) and re.fullmatch(r"ami-[a-f0-9]{17}", ami):
        checks["controller_ami_inspection"] = aws_read([
            "ec2", "describe-images", "--image-ids", ami, "--owners", "amazon",
            "--query", "Images[].{id:ImageId,owner:OwnerId,state:State,architecture:Architecture,created:CreationDate,root:RootDeviceName}",
        ], region)
    identity = checks["identity"].get("data", {})
    role = re.fullmatch(r"arn:aws:sts::[0-9]{12}:assumed-role/([^/]+)/[^/]+", identity.get("Arn", ""))
    if role:
        reads = {
            "role": ["iam", "get-role", "--role-name", role[1], "--query", "Role.{arn:Arn,boundary:PermissionsBoundary}"],
            "attached_policies": ["iam", "list-attached-role-policies", "--role-name", role[1]],
            "inline_policies": ["iam", "list-role-policies", "--role-name", role[1]],
        }
        with ThreadPoolExecutor(max_workers=3) as pool:
            checks.update(pool.map(lambda item: (item[0], aws_read(item[1], region)), reads.items()))
    return {"checked_at": datetime.now(timezone.utc).isoformat(), "region": region,
            "read_only": True, "deployment_permission_proven": False,
            "checks": checks, "scope": "Selected service inventory; API success is not proof of compatible AMI/engine/quotas or deployment permission. Not a billing audit."}


def write_private(path: Path, value: dict) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    # Refuse symlinks and overwriting existing evidence, including other runs.
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "w") as output:
        json.dump(value, output, indent=2)
        output.write("\n")
