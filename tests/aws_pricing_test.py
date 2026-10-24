"""Pricing ambiguity, units and missing rates must not become a fake complete estimate."""
from decimal import Decimal
from datetime import datetime, timezone
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from aws_preflight.discovery import aws_read, write_private
from aws_preflight.pricing import QUERIES, estimate, quote, rates_from_products


def product(rate="0.10", unit="Hrs"):
    return json.dumps({"product": {"sku": "test-sku"}, "terms": {"OnDemand": {"term": {
        "effectiveDate": "2026-10-01", "priceDimensions": {"rate": {
            "unit": unit, "description": "test rate", "beginRange": "0", "endRange": "Inf", "pricePerUnit": {"USD": rate}}}}}}})


def catalog(rates=None):
    return {"region": "ap-south-1", "currency": "USD", "checked_at": datetime.now(timezone.utc).isoformat(), "rates": rates or {}}


class PricingTests(unittest.TestCase):
    def test_ambiguous_missing_and_denied_pricing_fail_closed(self):
        for response in [{"status": "pass", "data": {"PriceList": []}},
                         {"status": "pass", "data": {"PriceList": [product(), product()]}},
                         {"status": "blocked", "reason": "AccessDenied"}]:
            with patch("aws_preflight.pricing.aws_read", return_value=response):
                self.assertEqual(quote("database")[1]["status"], "blocked")

    def test_exact_positive_rate_retains_sku_and_effective_date(self):
        rates = rates_from_products([product("0.042")])
        self.assertEqual(rates[0]["sku"], "test-sku")
        self.assertEqual(rates[0]["usd"], "0.042")
        self.assertEqual(rates[0]["effective_date"], "2026-10-01")

    def test_missing_units_do_not_produce_a_complete_estimate(self):
        result = estimate(catalog(), Decimal(1))
        self.assertFalse(result["complete_rate_set"])
        self.assertEqual(len(result["missing_rates"]), len(QUERIES))
        self.assertFalse(result["hard_spend_cap"])
        result = estimate(catalog({"controller": {"status": "pass", "usd": "1", "unit": "GB", "sku": "x"}}), Decimal(1))
        self.assertIn("controller", result["missing_rates"])

    def test_invalid_or_unbounded_resource_windows_fail(self):
        for hours in ("0", "-1", "25", "NaN", "Infinity"):
            with self.assertRaises(ValueError):
                estimate(catalog(), Decimal(hours))
        for price in ("NaN", "Infinity", "-1"):
            with self.assertRaises(ValueError):
                estimate(catalog({"controller": {"status": "pass", "usd": price, "unit": "Hrs", "sku": "x"}}), Decimal(1))

    def test_no_default_free_rate_when_one_component_is_unknown(self):
        result = estimate(catalog({"controller": {"status": "pass", "usd": "1", "unit": "Hrs", "sku": "x"}}), Decimal(1))
        self.assertEqual(result["estimated_subtotal_usd"], "1.000000")
        self.assertFalse(result["complete_rate_set"])
        self.assertIn("database", result["missing_rates"])

    def test_wrong_region_stale_or_unlabelled_catalog_fails(self):
        for change in ({"region": "us-east-1"}, {"currency": "INR"}, {"checked_at": "2020-01-01T00:00:00+00:00"},
                       {"checked_at": "2026-10-24T00:00:00"}):
            with self.assertRaises(ValueError):
                estimate({**catalog(), **change}, Decimal(1))

    def test_mutating_aws_operations_are_rejected_before_subprocess(self):
        with patch("aws_preflight.discovery.subprocess.run") as run:
            for arguments in (["ec2", "terminate-instances"], ["ssm", "get-parameter", "--name", "private-secret", "--with-decryption"]):
                with self.assertRaises(ValueError):
                    aws_read(arguments, "ap-south-1")
            run.assert_not_called()

    def test_private_evidence_does_not_overwrite_or_follow_symlinks(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "report.json"
            write_private(target, {"pass": True})
            self.assertEqual(target.stat().st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError):
                write_private(target, {})
            link = Path(directory) / "link"
            link.symlink_to(target)
            with self.assertRaises(FileExistsError):
                write_private(link, {})

    def test_cli_denial_never_echoes_an_upstream_secret(self):
        class Denied:
            returncode = 1
            stderr = "An error occurred (AccessDenied) SECRET_SENTINEL"
        with patch("aws_preflight.discovery.subprocess.run", return_value=Denied()) as run:
            result = aws_read(["sts", "get-caller-identity"], "ap-south-1")
            self.assertNotIn("SECRET_SENTINEL", json.dumps(result))
            self.assertEqual(result["reason"], "AccessDenied")
            self.assertIsInstance(run.call_args.args[0], list)


if __name__ == "__main__":
    unittest.main()
