"""Windows packaging must reject stale or mismatched final assets before publication."""
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

RELEASE = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('package_windows', RELEASE / 'package-windows.py')
PACKAGER = importlib.util.module_from_spec(SPEC)
with patch.object(sys, 'path', [str(RELEASE), *sys.path]):
    SPEC.loader.exec_module(PACKAGER)
    from verify_desktop import smoke_payload


@unittest.skipUnless(os.name == 'nt', 'native Windows staging fixture')
class WindowsMenuGateTests(unittest.TestCase):
    def test_no_payload_is_published_until_the_staged_menu_is_current(self):
        # Existing real Windows acceptance covers the executable and real menu.
        # A few bytes exercise every local staging/rollback branch without a bake.
        with tempfile.TemporaryDirectory() as temporary:
            # Match the packager's resolved destinations if Windows TEMP uses an 8.3 alias.
            root = Path(temporary).resolve(strict=True)
            source = root / 'checkout'
            bins = root / 'native bins'
            runtime = root / 'runtime'
            for folder in ('docs', 'assets/shared/menu', 'assets/audio', 'assets/environments',
                           'assets/entities', 'assets/diagnostic', 'assets/editor', 'projects/menu'):
                (source / folder).mkdir(parents=True)
            for directory in ('environments', 'editor'):
                (source / 'assets' / directory / 'master.xcf').write_bytes(b'editable source')
                (source / 'assets' / directory / 'native.png').write_bytes(b'runtime resource')
            bins.mkdir()
            (runtime / 'licenses').mkdir(parents=True)
            for name in ('README.md', 'LICENSE', 'THIRD_PARTY_LICENSES.txt', 'docs/EDITOR.md',
                         'docs/INSTALLATION.md', 'assets/catalog.json', 'assets/gameplay.json'):
                (source / name).write_text('fixture', encoding='utf-8')
            (source / 'Cargo.toml').write_text('[package]\nversion = "1.2.3"\n', encoding='utf-8')
            (source / 'assets/shared/menu/scene.bin').write_bytes(b'old checkout menu')
            prepared = root / 'editor menu.placesmap'
            prepared.write_bytes(b'prepared menu')
            license_file = runtime / 'SDL3-LICENSE.txt'
            license_file.write_text('test notice', encoding='utf-8')
            (runtime / 'licenses/notice.txt').write_text('test notice', encoding='utf-8')
            names = {'places.exe', 'places-map-compiler.exe', 'places-editor.exe'}
            # Test the actual DLL/binary header admission without running fake EXEs.
            pe = b'MZ' + bytes(58) + struct.pack('<I', 64) + b'PE\0\0\x64\x86'
            for name in names:
                (bins / name).write_bytes(pe)
            for name in PACKAGER.DLLS:
                (runtime / name).write_bytes(pe)
            for failure in ('dependencies', 'stale', None):
                with self.subTest(failure=failure):
                    destination = root / f'export {failure}'
                    calls = []

                    def native(command, **options):
                        calls.append(command[1])
                        self.assertTrue(Path(command[0]).samefile(bins / 'places-map-compiler.exe'))
                        assets = Path(command[command.index('--asset-root') + 1])
                        self.assertTrue(assets.is_relative_to(destination))
                        self.assertFalse(list(assets.rglob('*.xcf')))
                        for directory in ('environments', 'editor'):
                            self.assertEqual((assets / directory / 'native.png').read_bytes(),
                                             b'runtime resource')
                        self.assertEqual((assets / 'shared/menu/scene.bin').read_bytes(), prepared.read_bytes())
                        self.assertNotIn('PLACES_ASSET_ROOT', options['env'])
                        self.assertTrue(Path(options['env']['PLACES_STATE_ROOT']).is_dir())
                        if command[1] == 'validate':
                            self.assertEqual(Path(command[2]), assets / 'shared/menu/scene.bin')
                            data = json.dumps({'dependencies_intact': failure != 'dependencies'})
                            return subprocess.CompletedProcess(command, 0, data, '')
                        self.assertEqual(command[1], 'verify')
                        self.assertEqual(Path(command[2]), source / 'projects/menu')
                        self.assertIn('--require-current', command)
                        self.assertIn('places_menu', command)
                        self.assertEqual(Path(command[command.index('--package') + 1]),
                                         assets / 'shared/menu/scene.bin')
                        if failure == 'stale':
                            raise subprocess.CalledProcessError(1, command, stderr='stale compiler input')
                        return subprocess.CompletedProcess(command, 0, '{}', '')

                    argv = ['package-windows.py', str(destination), '--with-editor', '--bin-dir', str(bins),
                            '--runtime-dir', str(runtime), '--sdl-license', str(license_file),
                            '--menu-package', str(prepared)]
                    with patch.object(PACKAGER, 'ROOT', source), patch.object(sys, 'argv', argv), \
                            patch.object(PACKAGER.subprocess, 'run', side_effect=native), \
                            patch.object(PACKAGER.subprocess, 'check_output', return_value='source-sha\n'), \
                            patch.dict(os.environ, PLACES_ASSET_ROOT=str(root / 'wrong assets')):
                        if failure is None:
                            self.assertEqual(PACKAGER.main(), 0)
                        else:
                            reason = 'not intact' if failure == 'dependencies' else 'stale compiler input'
                            with self.assertRaisesRegex(ValueError, reason):
                                PACKAGER.main()
                    self.assertEqual(calls, ['validate'] if failure == 'dependencies' else ['validate', 'verify'])
                    self.assertFalse(list(destination.glob('.places-windows-stage-*')))
                    for directory in ('environments', 'editor'):
                        self.assertEqual((source / 'assets' / directory / 'master.xcf').read_bytes(),
                                         b'editable source')
                    payload = destination / 'Places'
                    if failure is None:
                        self.assertEqual({exe.name for exe in payload.glob('*.exe')}, names)
                        self.assertFalse(list((payload / 'assets/levels').iterdir()))
                        self.assertEqual((payload / 'assets/shared/menu/scene.bin').read_bytes(),
                                         prepared.read_bytes())
                        # Preserve installed discovery from outside the payload,
                        # not just when the source working directory is present.
                        cli = [('places.exe', '--list-levels'), ('places-editor.exe', '--help'),
                               ('places-map-compiler.exe', '--help')]
                        with patch('subprocess.run', return_value=subprocess.CompletedProcess([], 0, '', '')) as native:
                            smoke_payload(payload)
                        self.assertEqual(len(native.call_args_list), len(cli))
                        for call, (name, flag) in zip(native.call_args_list, cli, strict=True):
                            self.assertEqual(call.args[0], [payload / name, flag])
                            self.assertNotEqual(call.kwargs['cwd'], payload)
                            self.assertNotIn('PLACES_ASSET_ROOT', call.kwargs['env'])
                    else:
                        self.assertFalse(payload.exists(), 'failed validation must not publish')
