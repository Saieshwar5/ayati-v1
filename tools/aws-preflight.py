#!/usr/bin/env python3
"""Prepare M0 evidence locally. This command never deploys or changes AWS resources."""
import argparse
import json
from pathlib import Path
import sys
from decimal import Decimal, DecimalException

from aws_preflight.discovery import discover, write_private
from aws_preflight.pricing import capture, estimate
from aws_preflight.manifest import validate


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    audit = commands.add_parser("audit", help="Allowlisted AWS inventory and quota reads")
    audit.add_argument("--region", choices=["ap-south-1"], default="ap-south-1")
    audit.add_argument("--output", type=Path, required=True)
    prices = commands.add_parser("prices", help="Read current public AWS On-Demand rates")
    prices.add_argument("--output", type=Path, required=True)
    cost = commands.add_parser("estimate", help="Local candidate subtotal; never a spending cap")
    cost.add_argument("--prices", type=Path, required=True)
    cost.add_argument("--resource-hours", type=Decimal, required=True)
    cost.add_argument("--output", type=Path, required=True)
    review = commands.add_parser("review", help="Validate an inactive manifest and list exact resources")
    review.add_argument("--manifest", type=Path, required=True)
    review.add_argument("--allow-incomplete", action="store_true")
    review.add_argument("--output", type=Path, required=True)
    templates = commands.add_parser("templates", help="Check local boundaries; optionally call AWS ValidateTemplate")
    templates.add_argument("--aws", action="store_true")
    templates.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command in ("review", "templates"):
        from aws_preflight.templates import invariants, load_templates, resource_inventory, validate_with_aws
        loaded = load_templates()
        errors = invariants(loaded)
        report = {"template_errors": errors, "resources": resource_inventory(loaded), "deployment_ready": False}
        if args.command == "review":
            from aws_preflight.configuration import controller_environment, stack_parameters, stack_tags
            manifest = json.loads(args.manifest.read_text())
            report["manifest"] = validate(manifest)
            if report["manifest"]["schema_ok"]:
                report["stack_parameters"] = stack_parameters(manifest)
                report["stack_tags"] = stack_tags(manifest)
                report["controller_settings_contract"] = controller_environment(manifest)
            ok = not errors and report["manifest"]["schema_ok"] and (
                args.allow_incomplete or report["manifest"]["configuration_complete"])
        else:
            report["aws_validation"] = validate_with_aws() if args.aws else None
            ok = not errors and (not args.aws or all(r["status"] == "pass" for r in report["aws_validation"].values()))
        write_private(args.output, report)
        print(json.dumps({"evidence": str(args.output), "ok": ok,
                          "deployment_ready": False, "template_errors": errors,
                          "manifest": report.get("manifest"),
                          "aws_validation": {k: v["status"] for k, v in (report.get("aws_validation") or {}).items()}}, indent=2))
        return 0 if ok else 1
    if args.command in ("prices", "estimate"):
        report = capture() if args.command == "prices" else estimate(
            json.loads(args.prices.read_text()), args.resource_hours)
        write_private(args.output, report)
        if args.command == "prices":
            summary = {"rates": {key: value["status"] for key, value in report["rates"].items()}}
            ok = all(value["status"] == "pass" for value in report["rates"].values())
        else:
            summary, ok = report, report["complete_rate_set"]
        print(json.dumps({"evidence": str(args.output), **summary}, indent=2))
        return 0 if ok else 1
    report = discover(args.region)
    write_private(args.output, report)
    statuses = {key: value["status"] + (": " + value["reason"] if "reason" in value else "")
                for key, value in report["checks"].items()}
    print(json.dumps({"evidence": str(args.output), "checks": statuses}, indent=2))
    return 0 if report["checks"]["identity"]["status"] == "pass" else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError, DecimalException) as error:
        print(f"Preflight failed: {type(error).__name__}", file=sys.stderr)
        sys.exit(1)
