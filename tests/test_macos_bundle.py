"""Tiny linkage preflight regression: never publish a hidden Homebrew dependency.

Mocked otool output exercises paths with spaces and unsupported dependencies;
no Rust compilation, device, shipped map or native process is needed.
"""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location('bundle_macos', Path(__file__).resolve().parents[1] / 'tools/bundle-macos.py')
BUNDLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUNDLE)


class MacRuntimePreflightTests(unittest.TestCase):
    def test_minimum_os_is_read_from_linked_payload_not_assumed_from_rust(self):
        # Prevent advertising Rust's older deployment target when the bundled
        # SDL requires a newer OS. Canned load commands cost no native launch.
        with patch.object(BUNDLE, 'run', return_value='    minos 11.0\n    minos 27.0\n'):
            self.assertEqual(BUNDLE.minimum_macos(Path('SDL3')), '27.0')
        with patch.object(BUNDLE, 'run', return_value='no build version'):
            with self.assertRaisesRegex(ValueError, 'cannot determine minimum'):
                BUNDLE.minimum_macos(Path('SDL3'))

    def test_spaces_preserved_and_extra_dependencies_refused(self):
        with tempfile.TemporaryDirectory(prefix='places runtime ') as temporary:
            root = Path(temporary)
            sdl = root / 'libSDL3.0.dylib'
            sdl.write_bytes(b'fixture')
            player = root / 'places'
            apple = '/usr/lib/libSystem.B.dylib'

            def otool(*args):
                path = args[-1]
                names = [str(sdl), apple]
                return f'{path}:\n' + ''.join(f'\t{name} (compatibility version 1.0.0, current version 1.0.0)\n' for name in names)

            with patch.object(BUNDLE, 'run', side_effect=otool):
                source, links = BUNDLE.inspect([player])
                self.assertEqual(source, sdl.resolve())
                self.assertEqual(links[player], str(sdl))
            with patch.object(BUNDLE, 'dependencies', return_value=[str(sdl), '/opt/other/libbad.dylib']):
                with self.assertRaisesRegex(ValueError, 'expected only SDL3'):
                    BUNDLE.inspect([player])
            with patch.object(BUNDLE, 'dependencies', side_effect=[[str(sdl)], [str(sdl), '/opt/other/libbad.dylib']]):
                with self.assertRaisesRegex(ValueError, 'SDL3 has unsupported'):
                    BUNDLE.inspect([player])
