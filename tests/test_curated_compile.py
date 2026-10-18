"""Explicit export must check every sub-map without baking production content."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

from tests.built_maps import selected_distributions, selected_sources
from tools.levels.curated_maps import source_levels

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools/levels'))
SPEC = importlib.util.spec_from_file_location('curated_compile', ROOT / 'tools/levels/compile.py')
COMPILE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPILE)


def lighting_manifest(identity):
    return {'id': identity, 'variants': [
        {'lightmap_quality': quality, 'entries': {'lightmaps': 'atlas.pllc'}}
        for quality in ('off', 'full')]}


class CuratedCompileTests(unittest.TestCase):
    def test_default_acceptance_covers_every_lineup_source(self):
        with mock.patch.dict(os.environ):
            os.environ.pop('PLACES_PACKAGE_TEST_IDS', None)
            expected = [path for _, source in COMPILE.lineup() for _, path in source_levels(source)]
            self.assertEqual(selected_sources(), expected)
            self.assertTrue(expected)

    def test_archive_shards_reject_unknown_or_duplicate_distributions(self):
        identity, source = COMPILE.lineup()[0]
        for selected in ('', 'obsolete_dev_map', f'{identity},{identity}'):
            with mock.patch.dict(os.environ, PLACES_PACKAGE_TEST_IDS=selected):
                with self.assertRaises(ValueError):
                    selected_sources()
        with mock.patch.dict(os.environ, PLACES_PACKAGE_TEST_IDS=identity):
            self.assertEqual(selected_distributions(), [(identity, source)])
            self.assertEqual(selected_sources(), [path for _, path in source_levels(source)])

    def test_project_export_verifies_every_level_and_rejects_hidden_full_fallback(self):
        # Two tiny manifests protect bundle completeness and a non-entry map's
        # lighting fallback. No world preparation, renderer or real map is used.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            project = root / 'projects/environments'
            (project / 'levels').mkdir(parents=True)
            (root / 'levels').mkdir()
            (root / 'levels/lineup.txt').write_text('environments\n')
            manifest = {'schema': 'places-project', 'id': 'environments', 'entry_level': 'hub',
                        'levels': [{'id': identity, 'source': f'levels/{identity}.json'}
                                   for identity in ('hub', 'winter')]}
            source = project / 'places.project.json'
            source.write_text(json.dumps(manifest))
            for identity in ('hub', 'winter'):
                (project / f'levels/{identity}.json').write_text(json.dumps({'id': identity}))
            self.assertEqual(COMPILE.lineup(root), [('environments', source)])
            output = root / 'output'
            output.mkdir()
            package = output / 'environments.placesmap'
            for fallback in (False, True):
                with zipfile.ZipFile(package, 'w') as archive:
                    index = {'schema': 'places-project-package', 'package_format': 1,
                             'project': manifest, 'levels': []}
                    for identity in ('hub', 'winter'):
                        name = f'{identity}/manifest.json'
                        index['levels'].append({'id': identity, 'records': {'manifest.json': name}})
                        lighting = lighting_manifest(identity)
                        if fallback and identity == 'winter':
                            lighting['variants'][1]['entries']['lightmaps'] = None
                        archive.writestr(name, json.dumps(lighting))
                    archive.writestr('project.json', json.dumps(index))
                with mock.patch('sys.argv', ['compile.py', '--map', 'environments', '--output', str(output)]), \
                     mock.patch.object(COMPILE, 'lineup', return_value=[('environments', source)]), \
                     mock.patch.object(COMPILE.subprocess, 'run') as run, \
                     contextlib.redirect_stdout(io.StringIO()):
                    if fallback:
                        with self.assertRaisesRegex(ValueError, 'fell back'):
                            COMPILE.main()
                    else:
                        self.assertEqual(COMPILE.main(), 0)
                commands = [call.args[0] for call in run.call_args_list]
                self.assertEqual([command[1] for command in commands],
                                 ['export-project', 'verify', 'verify', 'validate'])
                self.assertIn(str(source), commands[0])
                self.assertNotIn('--variants', commands[0])
                self.assertEqual([command[command.index('--level')+1] for command in commands[1:3]],
                                 ['hub', 'winter'])
                self.assertTrue(all('--require-current' in command for command in commands[1:3]))

    def test_full_fallback_cannot_pass_curated_export(self):
        for failure in (None, 'page overflow'):
            manifest = {'variants': [{'lightmap_quality': 'off'},
                                    {'lightmap_quality': 'full', 'lightmap_failure': failure,
                                     'entries': {'lightmaps': None}}]}
            with self.assertRaisesRegex(ValueError, 'fell back'):
                COMPILE.validate_lighting(manifest, Path('fixture.placesmap'))

    def test_unknown_compile_shard_cannot_start_a_compiler(self):
        with mock.patch('sys.argv', ['compile.py', '--map', 'obsolete_dev_map']), \
             mock.patch.object(COMPILE.subprocess, 'run') as run, \
             contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as error:
                COMPILE.main()
        self.assertEqual(error.exception.code, 2)
        run.assert_not_called()


if __name__ == '__main__':
    unittest.main()
