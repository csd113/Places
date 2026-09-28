"""Filesystem-safety regression for tools/package.sh (audit AUD-001).

The output root is an export parent shared with unrelated files. These tests
prove the destination guard refuses the repository, the home directory and
their ancestors, that pre-existing unrelated siblings survive a real packaging
run, and that a symlinked product directory is refused rather than replaced.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PACKAGE_SH = ROOT / "tools" / "package.sh"
RELEASE_BINARY = ROOT / "target" / "release" / "places"


def check_destination(destination: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["sh", str(PACKAGE_SH), "--check-destination", str(destination)],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=False,
        timeout=60,
    )


class PackageDestinationGuardTests(unittest.TestCase):
    def test_dangerous_destinations_are_refused(self):
        cases = {
            "filesystem root": Path("/"),
            "home directory": Path.home(),
            "repository root": ROOT,
            "repository ancestor": ROOT.parent,
        }
        for label, destination in cases.items():
            with self.subTest(label=label):
                result = check_destination(destination)
                self.assertNotEqual(
                    result.returncode, 0, f"{label} must be refused: {result.stdout}"
                )
                self.assertIn("refusing", result.stdout.lower())

    def test_the_guard_creates_nothing(self):
        with tempfile.TemporaryDirectory() as directory:
            absent = Path(directory) / "new-export"
            result = check_destination(absent)
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertFalse(absent.exists(), "the check must not create the root")

    def test_symlinked_product_is_refused_without_touching_its_target(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "export"
            outside = Path(directory) / "outside"
            root.mkdir()
            outside.mkdir()
            target = outside / "keep-me.txt"
            target.write_text("unrelated")
            os.symlink(outside, root / "Places")
            result = check_destination(root)
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("symlink", result.stdout.lower())
            self.assertTrue(target.is_file(), "the symlink target must be untouched")

    def test_symlinked_root_cannot_disguise_a_protected_destination(self):
        # A logical `cd`/`pwd` would keep the link path, letting a symlinked
        # root bypass the home/repository refusal. The guard resolves the real
        # target, including through an intermediate link.
        with tempfile.TemporaryDirectory() as directory:
            home_link = Path(directory) / "home-link"
            os.symlink(Path.home(), home_link)
            repo_link = Path(directory) / "repo-link"
            os.symlink(ROOT, repo_link)
            chained = Path(directory) / "chained-link"
            os.symlink(home_link, chained)
            for label, destination in {
                "home symlink": home_link,
                "repository symlink": repo_link,
                "chained home symlink": chained,
            }.items():
                with self.subTest(label=label):
                    result = check_destination(destination)
                    self.assertNotEqual(result.returncode, 0, result.stdout)
                    self.assertIn("refusing", result.stdout.lower())

    def test_unrelated_siblings_survive_a_real_packaging_run(self):
        if os.environ.get("PLACES_SKIP_PACKAGING") == "1":
            self.skipTest("PLACES_SKIP_PACKAGING=1")
        if not RELEASE_BINARY.is_file():
            self.skipTest(
                "no release binary at target/release/places; "
                "run `cargo build --release` first"
            )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "export"
            root.mkdir()
            sentinel = root / "keep-me.txt"
            sentinel.write_text("unrelated data")
            unrelated_dir = root / "photos"
            unrelated_dir.mkdir()
            (unrelated_dir / "holiday.txt").write_text("also unrelated")

            result = subprocess.run(
                ["sh", str(PACKAGE_SH), str(root)],
                cwd=ROOT,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                check=False,
                timeout=900,
            )
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertTrue(sentinel.is_file(), "the sentinel file must survive")
            self.assertEqual(sentinel.read_text(), "unrelated data")
            self.assertTrue((unrelated_dir / "holiday.txt").is_file())
            self.assertTrue((root / "Places" / "places").is_file())
            self.assertTrue((root / "Places" / "levels").is_dir())
            self.assertTrue(
                (root / "Places.app" / "Contents" / "MacOS" / "places").is_file()
            )
            self.assertTrue(
                (
                    root
                    / "Places.app"
                    / "Contents"
                    / "Resources"
                    / "assets"
                    / "catalog.json"
                ).is_file()
            )


if __name__ == "__main__":
    unittest.main()
