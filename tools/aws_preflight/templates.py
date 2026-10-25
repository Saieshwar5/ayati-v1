"""Static template invariants and read-only AWS CloudFormation validation."""
from pathlib import Path

from .discovery import aws_read


DIRECTORY = Path(__file__).resolve().parents[2] / "deploy/aws"
ORDER = ("network", "data", "operations", "permissions", "controller", "executors", "edge")


def load_templates(directory: Path = DIRECTORY) -> dict:
    import yaml
    return {name: yaml.safe_load((directory / f"{name}.yaml").read_text()) for name in ORDER}


def invariants(templates: dict) -> list[str]:
    errors = []

    def check(allowed, message):
        if not allowed:
            errors.append(message)

    for template in templates.values():
        target = template.get("Rules", {}).get("DeploymentTarget", {}).get("Assertions", [])
        check(len(target) == 2 and target[0]["Assert"] == {"Fn::Equals": [{"Ref": "AWS::AccountId"}, {"Ref": "ExpectedAccountId"}]} and
              target[1]["Assert"] == {"Fn::Equals": [{"Ref": "AWS::Region"}, "ap-south-1"]},
              "Every stack must reject an unreviewed account or region")

    controller = templates["controller"]["Resources"]
    group = controller["Controllers"]["Properties"]
    check(all(str(group.get(name)) == "0" for name in ("MinSize", "MaxSize", "DesiredCapacity")),
          "M0 controller activation must remain zero")
    executor = templates["executors"]["Resources"]
    check(executor["Service"]["Properties"]["DesiredCount"] == 0, "M0 executor activation must remain zero")
    scaling = executor["Scaling"]["Properties"]
    check(scaling["MinCapacity"] == scaling["MaxCapacity"] == 0 and
          all(scaling["SuspendedState"].get(key) is True for key in (
              "DynamicScalingInSuspended", "DynamicScalingOutSuspended", "ScheduledScalingSuspended")),
          "M0 autoscaling must remain suspended at zero")
    definitions = executor["Task"]["Properties"]["ContainerDefinitions"]
    harness = next(item for item in definitions if item["Name"] == "executor")
    check(harness["User"] == "10001:10001" and harness["ReadonlyRootFilesystem"] is True,
          "Executor must retain its non-root/read-only boundary")
    check(harness["LinuxParameters"]["Capabilities"]["Drop"] == ["ALL"], "Executor must drop capabilities")
    names = set()
    for container in definitions:
        for item in container.get("Environment", []) + container.get("Secrets", []):
            name = item["Name"]
            if isinstance(name, str):
                names.add(name)
            elif isinstance(name, dict) and "Fn::If" in name:
                names.update(name["Fn::If"][1:])
            else:
                errors.append("Unrecognized credential-name expression")
    check(not names & {"RELAY_DATABASE_URL", "RELAY_DATABASE_URL_FILE", "WORKOS_API_KEY", "WORKOS_API_KEY_FILE",
                       "WORK_OS_API_KEY", "WORKOS_CLIENT_ID", "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN"},
          "Executor must not receive core or developer credentials")
    check(harness["Secrets"] == [{"Name": {"Fn::If": ["Openai", "OPENAI_API_KEY", "FIREWORKS_API_KEY"]},
                                  "ValueFrom": {"Ref": "ModelSecretArn"}}],
          "Only the exact configured model secret may enter the harness")
    check(all(item["Image"] == {"Ref": "ImageUri"} for item in definitions), "All task containers must use the same reviewed image digest")
    check(not definitions[0].get("Secrets") and not definitions[0].get("Environment"), "Scratch init must receive no configured credentials")
    check(harness["Command"] == ["executor"], "AWS must use prepared executors, not the old polling worker")
    check(executor["Task"]["Properties"]["Cpu"] == "2048" and executor["Task"]["Properties"]["Memory"] == "4096",
          "Task sizing must match the reviewed two-slot cost profile")
    for item in definitions:
        check(not item.get("Privileged", False), "No privileged container is allowed")
    data = templates["data"]["Resources"]
    db = data["Database"]["Properties"]
    check(db["PubliclyAccessible"] is False and db["StorageEncrypted"] is True and db["DeletionProtection"] is True,
          "RDS must be private, encrypted and deletion-protected")
    check(data["Database"]["DeletionPolicy"] == "Snapshot", "RDS deletion must retain a snapshot")
    bucket = data["Files"]
    check(bucket["DeletionPolicy"] == "Retain" and all(bucket["Properties"]["PublicAccessBlockConfiguration"].values()),
          "S3 must remain retained and public-access-blocked")
    check("LifecycleConfiguration" not in bucket["Properties"], "Do not expire live Library originals")
    roles = templates["permissions"]["Resources"]
    for resource in roles.values():
        if resource["Type"] == "AWS::IAM::Role":
            check(resource["Properties"].get("PermissionsBoundary") == {"Ref": "RuntimeBoundaryArn"},
                  "Every runtime role requires the reviewed external permissions boundary")
    policies = roles["ExecutorRole"]["Properties"].get("Policies", [])
    check(not any(s["Effect"] == "Allow" for p in policies for s in p["PolicyDocument"]["Statement"]),
          "M0 executor task role must not grant unproven cloud bootstrap authority")
    pass_role = roles["PassRolePolicy"]["Properties"]
    check(not pass_role.get("Roles") and not pass_role.get("Users") and not pass_role.get("Groups"),
          "The reviewed PassRole policy must remain unattached in M0")
    check(all(s["Action"] == "iam:PassRole" and s["Resource"] != "*" and
              "iam:PassedToService" in s.get("Condition", {}).get("StringEquals", {})
              for s in pass_role["PolicyDocument"]["Statement"]),
          "PassRole requires exact resources and target-service conditions")
    controller_statements = roles["ControllerRole"]["Properties"]["Policies"][0]["PolicyDocument"]["Statement"]
    secret_resources = [s["Resource"] for s in controller_statements if s["Action"] == "secretsmanager:GetSecretValue"]
    check(secret_resources == [[{"Ref": "RuntimeDatabaseSecretArn"}, {"Ref": "WorkosSecretArn"}]],
          "Controller must not receive administrator or model credentials")
    network = templates["network"]["Resources"]
    check(not network["ExecutorGroup"]["Properties"].get("SecurityGroupIngress"), "Executors must have no inbound listener")
    check(all(rule.get("FromPort") != 8766 for item in network.values() for rule in item.get("Properties", {}).get("SecurityGroupIngress", [])),
          "Loopback-only worker control must not be exposed before M3")
    check(not any(item["Type"] == "AWS::ApiGatewayV2::Route" for template in templates.values() for item in template["Resources"].values()),
          "An unverified cloud identity route must not be added to M0")
    return errors


def validate_with_aws(directory: Path = DIRECTORY) -> dict:
    return {name: aws_read(["cloudformation", "validate-template", "--template-body",
                            "file://" + str((directory / f"{name}.yaml").resolve())], "ap-south-1")
            for name in ORDER}


def resource_inventory(templates: dict) -> dict:
    return {name: [{"logical_id": key, "type": value["Type"],
                    "deletion_policy": value.get("DeletionPolicy", "Delete")}
                   for key, value in templates[name]["Resources"].items()] for name in ORDER}
