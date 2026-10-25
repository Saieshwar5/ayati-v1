"""Capture current Mumbai On-Demand USD rates without provisioning resources."""
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from decimal import Decimal
import json

from .discovery import aws_read


QUERIES = {
    "fargate_cpu": ("AmazonECS", {"usagetype": "APS3-Fargate-vCPU-Hours:perCPU"}),
    "fargate_memory": ("AmazonECS", {"usagetype": "APS3-Fargate-GB-Hours"}),
    "controller": ("AmazonEC2", {"instanceType": "t3.medium", "operatingSystem": "Linux",
                                "tenancy": "Shared", "preInstalledSw": "NA", "capacitystatus": "Used"}),
    "controller_disk": ("AmazonEC2", {"volumeApiName": "gp3", "productFamily": "Storage"}),
    "database": ("AmazonRDS", {"instanceType": "db.t4g.small", "databaseEngine": "PostgreSQL",
                              "deploymentOption": "Single-AZ", "usagetype": "APS3-InstanceUsage:db.t4g.small"}),
    "database_disk": ("AmazonRDS", {"databaseEngine": "PostgreSQL", "deploymentOption": "Single-AZ",
                                   "volumeType": "General Purpose-GP3", "productFamily": "Database Storage"}),
    "nat": ("AmazonEC2", {"usagetype": "APS3-NatGateway-Hours"}),
    "nat_data": ("AmazonEC2", {"usagetype": "APS3-NatGateway-Bytes"}),
    "alb": ("AWSELB", {"usagetype": "APS3-LoadBalancerUsage", "operation": "LoadBalancing:Application"}),
    "alb_lcu": ("AWSELB", {"usagetype": "APS3-LCUUsage", "operation": "LoadBalancing:Application"}),
    "public_ipv4": ("AmazonVPC", {"usagetype": "APS3-PublicIPv4:InUseAddress"}),
    "secret": ("AWSSecretsManager", {"usagetype": "APS3-AWSSecretsManager-Secrets"}),
}
SOURCE = "https://docs.aws.amazon.com/awsaccountbilling/latest/aboutv2/price-changes.html"


def rates_from_products(products: list[str]) -> list[dict]:
    rates = []
    for encoded in products:
        product = json.loads(encoded)
        for term in product.get("terms", {}).get("OnDemand", {}).values():
            for dimension in term["priceDimensions"].values():
                price = dimension.get("pricePerUnit", {}).get("USD")
                if price is not None and Decimal(price) > 0:
                    rates.append({"sku": product["product"]["sku"], "usd": price,
                                  "unit": dimension["unit"], "description": dimension["description"],
                                  "begin": dimension["beginRange"], "end": dimension["endRange"],
                                  "effective_date": term["effectiveDate"]})
    return rates


def quote(name: str) -> tuple[str, dict]:
    service, attributes = QUERIES[name]
    filters = [{"Type": "TERM_MATCH", "Field": field, "Value": value}
               for field, value in {"regionCode": "ap-south-1", **attributes}.items()]
    result = aws_read(["pricing", "get-products", "--service-code", service,
                       "--filters", json.dumps(filters)], "us-east-1", timeout=40)
    if result["status"] != "pass":
        return name, result
    try:
        rates = rates_from_products(result["data"]["PriceList"])
    except (ValueError, KeyError, TypeError):
        return name, {"status": "blocked", "reason": "InvalidPricingResponse"}
    if len(rates) != 1 or rates[0]["begin"] != "0" or rates[0]["end"] != "Inf":
        return name, {"status": "blocked", "reason": "AmbiguousOrMissingRate", "candidates": rates}
    return name, {"status": "pass", **rates[0], "service": service, "filters": attributes}


def capture() -> dict:
    with ThreadPoolExecutor(max_workers=4) as pool:
        rates = dict(pool.map(quote, QUERIES))
    return {"checked_at": datetime.now(timezone.utc).isoformat(), "region": "ap-south-1",
            "currency": "USD", "source": SOURCE, "read_only": True, "rates": rates,
            "limits": "Public On-Demand rates, not an invoice, allowance or account-wide billing audit."}


def estimate(catalog: dict, resource_hours: Decimal) -> dict:
    if catalog.get("region") != "ap-south-1" or catalog.get("currency") != "USD":
        raise ValueError("Estimate requires a Mumbai USD catalog")
    try:
        checked = datetime.fromisoformat(catalog["checked_at"])
        age = (datetime.now(timezone.utc) - checked).total_seconds()
        if not -300 <= age <= 86400:
            raise ValueError()
    except (KeyError, TypeError, ValueError):
        raise ValueError("Re-capture pricing; a timezone-aware catalog no older than 24 hours is required") from None
    if not resource_hours.is_finite() or not Decimal("0.25") <= resource_hours <= 24:
        raise ValueError("Resource window must be 0.25-24 hours")
    # Two two-slot executors match the recorded four-slot load profile. This
    # estimates a future trial, never the dormant M0 stack's running compute.
    hourly_floor = max(resource_hours, Decimal(1))
    quantities = {
        "controller": resource_hours, "controller_disk": 20 * resource_hours / 730,
        "database": resource_hours, "database_disk": 20 * resource_hours / 730,
        "fargate_cpu": 2 * 2 * resource_hours,
        "fargate_memory": 2 * 4 * resource_hours,
        "nat": hourly_floor, "nat_data": Decimal("0.25"),
        "alb": hourly_floor, "alb_lcu": hourly_floor,
        "public_ipv4": 3 * resource_hours, "secret": 4 * resource_hours / 730,
    }
    expected_units = {
        "controller": {"Hrs"}, "controller_disk": {"GB-Mo"}, "database": {"Hrs"},
        "database_disk": {"GB-Mo"}, "fargate_cpu": {"hours"}, "fargate_memory": {"hours"},
        "nat": {"Hrs"}, "nat_data": {"GB"}, "alb": {"Hrs"}, "alb_lcu": {"LCU-Hrs"},
        "public_ipv4": {"Hrs"}, "secret": {"Secrets"},
    }
    total = Decimal(0)
    lines, missing = [], []
    for name, quantity in quantities.items():
        rate = catalog.get("rates", {}).get(name, {})
        if rate.get("status") != "pass" or rate.get("unit") not in expected_units[name]:
            missing.append(name)
            continue
        price = Decimal(rate["usd"])
        if not price.is_finite() or price <= 0:
            raise ValueError("Invalid USD price")
        cost = quantity * price
        total += cost
        lines.append({"component": name, "quantity": str(quantity), "unit": rate["unit"],
                      "usd_rate": str(price), "usd": str(cost.quantize(Decimal("0.000001"))), "sku": rate["sku"]})
    return {"resource_window_hours": str(resource_hours), "active_test_minutes": 15,
            "pricing_checked_at": catalog["checked_at"],
            "estimated_subtotal_usd": str(total.quantize(Decimal("0.000001"))),
            "complete_rate_set": not missing, "missing_rates": missing, "lines": lines,
            "assumptions": ["One t3.medium controller in standard CPU-credit mode and one Single-AZ db.t4g.small.",
                            "Two 2-vCPU/4-GiB executors, two slots each; a fifth simultaneous agent request queues.",
                            "20 GiB gp3 on controller and RDS, four secrets, at least three public IPv4 addresses.",
                            "One NAT and ALB, at least one billed hour each, one LCU and 0.25 GB NAT processing.",
                            "730-hour month only for prorating monthly storage/secret rates; resource window includes setup/cleanup."],
            "excluded_unbounded_costs": ["Model calls and provider charges", "Internet/inter-zone transfer",
                                          "RDS surplus CPU credits",
                                          "S3/ECR storage and requests", "Logs, secret API requests and DNS",
                                          "Retained backups, versions, secrets and other resources after teardown",
                                          "Extra scale-out, deployment overlap or cleanup delay", "Taxes"],
            "budget_approved": False, "hard_spend_cap": False,
            "warning": "Subtotal for stated assumptions, not total cost or a guaranteed ceiling. Price exclusions and set a numeric budget before provisioning."}
