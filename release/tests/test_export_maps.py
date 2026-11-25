"""Optional map packaging must match native discovery and release policy."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock
import zipfile

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('export_maps', ROOT / 'release/export_maps.py')
EXPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EXPORT)


class ExternalMapExportTests(unittest.TestCase):
    def test_runtime_export_checks_project_members_instead_of_package_count(self):
        # Tiny ZIP metadata catches the nine-members/one-package mismatch and
        # missing or substituted discoveries without native resources or baking.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, payload = root / 'maps', root / 'payload'
            source.mkdir()
            (payload / 'assets/levels').mkdir(parents=True)
            package = source / 'collection.placesmap'
            with zipfile.ZipFile(package, 'w') as archive:
                archive.writestr('project.json', json.dumps({'project': {'id': 'environments'},
                                                            'levels': [{'id': 'hub'}, {'id': 'home'}]}))
            for identities in (('hub', 'home'), ('hub',), ('hub', 'hub')):
                results = [subprocess.CompletedProcess([], 0, json.dumps({'dependencies_intact': True})),
                           subprocess.CompletedProcess([], 0, ''.join(f'{identity}\tbundled\tpackage\n'
                                                                     for identity in identities), '')]
                with mock.patch.object(EXPORT.subprocess, 'run', side_effect=results), \
                     contextlib.redirect_stdout(io.StringIO()):
                    if identities == ('hub', 'home'):
                        EXPORT.export_maps(source, payload, root / 'compiler')
                        self.assertEqual((payload / 'assets/levels' / package.name).read_bytes(), package.read_bytes())
                    else:
                        with self.assertRaisesRegex(ValueError, 'differ from selected members'):
                            EXPORT.export_maps(source, payload, root / 'compiler')

    def test_archived_exports_refuse_renamed_projects_and_embedded_members_but_keep_beam(self):
        # A few hundred bytes protect against stale export directories and
        # filename-only filtering. Native validation is mocked; no bake or GPU.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, payload = root / 'maps', root / 'payload'
            source.mkdir()
            destination = payload / 'assets/levels'
            destination.mkdir(parents=True)
            policy = json.loads((ROOT / 'release/release-projects.json').read_text(encoding='utf-8'))
            for row in policy['archived_projects']:
                for kind in ('filename', 'project', 'member', 'standalone'):
                    with self.subTest(project=row['project_id'], kind=kind):
                        package = source / (row['package'] if kind == 'filename' else 'renamed.placesmap')
                        with zipfile.ZipFile(package, 'w') as archive:
                            if kind == 'standalone':
                                archive.writestr('manifest.json', json.dumps({'id': row['level_ids'][0]}))
                            else:
                                archive.writestr('project.json', json.dumps({
                                    'project': {'id': row['project_id'] if kind == 'project' else 'other'},
                                    'levels': [{'id': row['level_ids'][0] if kind == 'member' else 'other'}]}))
                        original = package.read_bytes()
                        with mock.patch.object(EXPORT.subprocess, 'run', return_value=
                                subprocess.CompletedProcess([], 0, '{"dependencies_intact":true}', '')) as native:
                            with self.assertRaisesRegex(ValueError, '[Aa]rchived'):
                                EXPORT.export_maps(source, payload, root / 'compiler')
                            self.assertEqual(native.call_count, 0 if kind == 'filename' else 1)
                        self.assertEqual(package.read_bytes(), original)
                        self.assertEqual(list(destination.iterdir()), [])
                        package.unlink()
            beam = json.loads((ROOT / 'projects/beam/places.project.json').read_text(encoding='utf-8'))
            package = source / 'beam.placesmap'
            with zipfile.ZipFile(package, 'w') as archive:
                archive.writestr('project.json', json.dumps({'project': beam, 'levels': beam['levels']}))
            results = [subprocess.CompletedProcess([], 0, '{"dependencies_intact":true}', ''),
                       subprocess.CompletedProcess([], 0, ''.join(row['id'] + '\tinstalled\n' for row in beam['levels']), '')]
            with mock.patch.object(EXPORT.subprocess, 'run', side_effect=results), \
                    contextlib.redirect_stdout(io.StringIO()):
                EXPORT.export_maps(source, payload, root / 'compiler')
            self.assertEqual((destination / package.name).read_bytes(), package.read_bytes())
