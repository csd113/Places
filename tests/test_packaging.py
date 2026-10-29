"""Filesystem-safety regression for tools/package.sh (audit AUD-001).

The output root is an export parent shared with unrelated files. These tests
prove the destination guard refuses the repository, the home directory and
their ancestors, that pre-existing unrelated siblings survive a real packaging
run, and that a symlinked product directory is refused rather than replaced.
"""

from __future__ import annotations

import os
import shutil
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
    def directory_symlink(self, target, link):
        try:
            os.symlink(target, link, target_is_directory=True)
        except OSError as error:
            if getattr(error, 'winerror', None) == 1314:
                self.skipTest('Windows directory-symlink privilege is unavailable; no host policy is changed')
            raise

    def test_map_free_export_needs_only_the_player_and_runtime_assets(self):
        # A tiny fake release player protects script dependency boundaries on
        # every CI run, without Rust compilation, SDL or a production-map bake.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'source'
            (source / 'tools').mkdir(parents=True)
            shutil.copyfile(PACKAGE_SH, source / 'tools/package.sh')
            # Script-only payload fixture: native dependency relocation is
            # checked separately with mocked otool and by explicit macOS QA.
            (source / 'tools/bundle-macos.py').write_text('import sys\nassert len(sys.argv) == 3\n')
            (source / 'docs').mkdir()
            (source / 'docs/INSTALLATION.md').write_text('installation fixture')
            (source / 'docs/EDITOR.md').write_text('editor fixture')
            (source / 'Cargo.toml').write_text('[package]\nversion = "0.9.0"\n')
            for name in ('README.md', 'THIRD_PARTY_LICENSES.txt'):
                (source / name).write_text('fixture')
            for name in ('shared', 'environments', 'entities', 'diagnostic', 'janitor_closet', 'levels', 'audio', 'editor'):
                (source / 'assets' / name).mkdir(parents=True)
            # Pool's catalog resources live outside environments/. Check their
            # byte custody in this tiny fixture instead of baking the collection.
            janitor = source / 'assets/janitor_closet/models/fixture.glb'
            janitor.parent.mkdir()
            janitor.write_bytes(b'janitor runtime fixture')
            runtime_metadata = {
                'catalog.json': b'{"assets": []}\n',
                'gameplay.json': b'{"physics": {"fixture:crate": {"mass": 2}}}\n',
            }
            for name, data in runtime_metadata.items():
                (source / 'assets' / name).write_bytes(data)
            # Protect newly required sound payload/credit custody in the existing
            # tiny export fixture; no device, decoder or map bake is involved.
            audio_payload = {'catalog.json': b'{"format_version":1,"cues":[]}\n',
                             'fixture.wav': b'RIFF fixture bytes', 'credits.txt': b'CC0 source credit'}
            for name, data in audio_payload.items():
                (source / 'assets/audio' / name).write_bytes(data)
            (source / 'assets/editor/fixture.png').write_bytes(b'editor preview fixture')
            (source / 'assets/completed-models.json').write_text('{"authoring": "only"}')
            (source / 'assets/shared/branding').mkdir()
            (source / 'assets/shared/branding/places.png').write_bytes(b'source artwork')
            (source / 'assets/environments/source.blend').write_bytes(b'unique model authoring source')
            (source / 'assets/environments/CABINET_ARTWORK_PROMPTS.md').write_text('source prompts')
            # Even an existing optional package is excluded unless requested.
            (source / 'assets/levels/unselected.placesmap').write_bytes(b'not a map')
            player = source / 'target/release/places'
            player.parent.mkdir(parents=True)
            player.write_text('#!/bin/sh\nexit 0\n')
            player.chmod(0o755)
            output = root / 'export'
            output.mkdir()
            (output / 'keep.txt').write_text('unique sibling')
            result = subprocess.run(['sh', str(source / 'tools/package.sh'), str(output)],
                                    text=True, capture_output=True, timeout=30, check=False)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual((output / 'keep.txt').read_text(), 'unique sibling')
            self.assertFalse((source / 'levels').exists())
            self.assertFalse((source / 'target/release/places-compile').exists())
            for payload in (output / 'Places', output / 'Places.app/Contents/Resources'):
                for name, data in runtime_metadata.items():
                    self.assertEqual((payload / 'assets' / name).read_bytes(), data)
                for name, data in audio_payload.items():
                    self.assertEqual((payload / 'assets/audio' / name).read_bytes(), data)
                self.assertEqual((payload / 'assets/janitor_closet/models/fixture.glb').read_bytes(),
                                 janitor.read_bytes())
                self.assertFalse((payload / 'assets/completed-models.json').exists())
                self.assertFalse((payload / 'assets/shared/branding').exists())
                self.assertFalse((payload / 'assets/environments/source.blend').exists())
                self.assertFalse((payload / 'assets/environments/CABINET_ARTWORK_PROMPTS.md').exists())
                self.assertFalse((payload / 'levels').exists(), 'writable levels belong in user storage')
                self.assertEqual((payload / 'docs/INSTALLATION.md').read_text(), 'installation fixture')
                self.assertEqual(list(payload.rglob('*.placesmap')), [])
            compiler = source / 'target/release/places-compile'
            compiler.write_text('#!/bin/sh\nexit 0\n')
            compiler.chmod(0o755)
            with_compiler = subprocess.run(['sh', str(source / 'tools/package.sh'), '--with-compiler', str(output)],
                                           text=True, capture_output=True, timeout=30, check=False)
            self.assertEqual(with_compiler.returncode, 0, with_compiler.stdout + with_compiler.stderr)
            for binary in (output / 'Places/places-compile', output / 'Places.app/Contents/MacOS/places-compile'):
                self.assertEqual(binary.read_bytes(), compiler.read_bytes())
            editor = source / 'target/release/places-editor'
            editor.write_text('#!/bin/sh\nexit 0\n')
            editor.chmod(0o755)
            with_editor = subprocess.run(['sh', str(source / 'tools/package.sh'), '--with-editor', str(output)],
                                         text=True, capture_output=True, timeout=30, check=False)
            self.assertEqual(with_editor.returncode, 0, with_editor.stdout + with_editor.stderr)
            for binary in (output / 'Places/places-editor', output / 'Places.app/Contents/MacOS/places-editor'):
                self.assertEqual(binary.read_bytes(), editor.read_bytes())
            for payload in (output / 'Places', output / 'Places.app/Contents/Resources'):
                self.assertEqual((payload / 'docs/EDITOR.md').read_text(), 'editor fixture')
                self.assertEqual((payload / 'assets/editor/fixture.png').read_bytes(), b'editor preview fixture')
            # Force the second publication rename to fail. Both previous
            # products must survive, not just unrelated export-root siblings.
            for product in ('Places', 'Places.app'):
                (output / product / 'previous.txt').write_text(product)
            commands = root / 'commands'
            commands.mkdir()
            move = commands / 'mv'
            # A shell fixture has no interpreter-path/shebang ambiguity on Git
            # for Windows, and still injects the same second-rename failure.
            move.write_text('#!/bin/sh\n'
                            'parent=${1%/*}; case "${parent##*/}/${1##*/}" in .places-package-stage.*/Places.app) exit 7;; esac\n'
                            'exec /bin/mv "$@"\n', encoding='utf-8', newline='\n')
            move.chmod(0o755)
            # Resolve the injected PATH entry inside the shell. A Windows
            # drive colon is not a POSIX PATH separator, even under Git sh.
            failed = subprocess.run(['sh', '-c',
                                     'PATH="$(cd "$1" && pwd -P):$PATH"; export PATH; shift; exec sh "$@"',
                                     'package-rollback-probe', str(commands),
                                     str(source / 'tools/package.sh'), '--with-editor', str(output)],
                                    text=True, capture_output=True, timeout=30, check=False)
            self.assertNotEqual(failed.returncode, 0)
            for product in ('Places', 'Places.app'):
                self.assertEqual((output / product / 'previous.txt').read_text(), product)
            self.assertEqual((output / 'keep.txt').read_text(), 'unique sibling')
            self.assertEqual(list(output.glob('.places-package-stage.*')), [])

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
            self.directory_symlink(outside, root / "Places")
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
            self.directory_symlink(Path.home(), home_link)
            repo_link = Path(directory) / "repo-link"
            self.directory_symlink(ROOT, repo_link)
            chained = Path(directory) / "chained-link"
            self.directory_symlink(home_link, chained)
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
        if os.environ.get("PLACES_TEST_EXPORT") != "1":
            self.skipTest("explicit native export acceptance: set PLACES_TEST_EXPORT=1 after building")
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
            self.assertFalse((root / "Places" / "levels").exists())
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
