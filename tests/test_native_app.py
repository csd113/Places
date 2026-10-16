"""Small filesystem fixtures protecting the native validation identity contract."""
from pathlib import Path
import plistlib
import tempfile
import unittest

from tools.bench import native_app


class NativeAppIdentityTests(unittest.TestCase):
    def test_result_directories_do_not_change_the_app_identity(self):
        # Protects the reproduced per-output bundle-ID/path churn. No SDL,
        # executable launch, asset scan, map compilation or Rust build.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "places"
            binary.write_bytes(b"small native identity fixture")
            directory = root / "validation"
            first = native_app.prepare_app(binary, directory)
            files = [p for p in directory.rglob("*") if p.is_file()]
            original = {str(p): (p.read_bytes(), p.stat().st_mtime_ns) for p in files}
            commands = []
            for name in ("baseline", "candidate"):
                identity = native_app.prepare_app(binary, directory)
                self.assertEqual(identity, first)
                commands.append(native_app.launch_command(identity, root / name, {
                    "PLACES_STATE_ROOT": str(root / name),
                }))
            self.assertEqual(original, {str(p): (p.read_bytes(), p.stat().st_mtime_ns) for p in files})
            self.assertEqual(commands[0][-1], commands[1][-1])
            self.assertNotEqual(commands[0], commands[1])
            self.assertEqual(first["bundle_identifier"], "io.github.csd113.places.validation")

    def test_an_unrelated_existing_bundle_is_not_replaced(self):
        # Protects the promise not to reuse another program's identity/grants;
        # the fixture is two tiny files and never contacts the permission system.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / "places"
            binary.write_bytes(b"fixture")
            plist = root / native_app.APP_NAME / "Contents/Info.plist"
            plist.parent.mkdir(parents=True)
            original = plistlib.dumps({"CFBundleIdentifier": "example.other.app"})
            plist.write_bytes(original)
            with self.assertRaisesRegex(ValueError, "unrelated app"):
                native_app.prepare_app(binary, root)
            self.assertEqual(plist.read_bytes(), original)
