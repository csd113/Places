"""Reject incomplete native scope and unclassified geometry errors before acceptance."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("native_regression_maps", ROOT / "tools/bench/native_regression_maps.py")
NATIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NATIVE)


class NativeMapCampaignContracts(unittest.TestCase):
    def test_native_scope_requires_every_successful_source_and_real_campaign_package(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            campaign, manifests = root / "campaign", root / "manifests"
            campaign.mkdir()
            manifests.mkdir()
            source, package = root / "source.json", root / "actual.placesmap"
            source.write_text('{"id":"same"}')
            package.write_bytes(b"actual package path")
            row = dict(key="one", source="source.json", level="same", classification="playable",
                       source_sha256=NATIVE.digest(source))
            (campaign / "inventory.json").write_text(json.dumps(dict(maps=[row])))
            (campaign / "plan.json").write_text(json.dumps([dict(key="one", commands=[[
                "/compiler", "build", str(source), "--out", str(package)]])]))
            (campaign / "package-results.json").write_text(json.dumps(dict(
                compiler_stable=True, failures=[], cases=[dict(key="one", status="passed")])))
            camera = dict(source="source.json", level="same", views=[dict(name="spawn")],
                          package="wrong manifest default")
            (manifests / "one.json").write_text(json.dumps(camera))
            self.assertEqual(NATIVE.plan(campaign, manifests, root)[0]["package"], str(package.resolve()))
            (manifests / "unexpected.json").write_text(json.dumps(camera))
            with self.assertRaises(ValueError):
                NATIVE.plan(campaign, manifests, root)
            (manifests / "unexpected.json").unlink()
            source.write_text('{"id":"changed"}')
            with self.assertRaises(ValueError):
                NATIVE.plan(campaign, manifests, root)

    def report(self, level, checks, validated=True):
        return dict(format="places-geometry-check", level=dict(id=level, validated=validated),
                    summary=dict(errors=sum(severity == "Error" for _, severity in checks)),
                    findings=[dict(check=check, severity=severity, message="test witness")
                              for check, severity in checks])

    def test_ordinary_geometry_errors_and_wrong_exit_codes_remain_failures(self):
        report = self.report("normal", [])
        NATIVE.check_geometry("normal.json", "normal", report, 0)
        for value, code in ((self.report("normal", [("duplicate-surface", "Error")]), 1),
                            (report, 2), (report, 1)):
            with self.assertRaises(ValueError):
                NATIVE.check_geometry("normal.json", "normal", value, code)

    def test_planted_findings_must_remain_named_without_accepting_unrelated_errors(self):
        source = "tests/fixtures/levels/geometry_broken.json"
        checks = [(check, "Error") for check in NATIVE.EXPECTED_ERRORS[source]]
        checks += [(check, "Warning") for check in NATIVE.EXPECTED_WARNINGS[source]]
        report = self.report("broken", checks)
        NATIVE.check_geometry(source, "broken", report, 1)
        for mutation in (lambda value: value["findings"].pop(),
                         lambda value: value["findings"].append(dict(check="ghost-collider", severity="Error"))):
            value = copy.deepcopy(report)
            mutation(value)
            with self.assertRaises(ValueError):
                NATIVE.check_geometry(source, "broken", value, 1)


if __name__ == "__main__":
    unittest.main()
