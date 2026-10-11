"""CI shards must cover curated sources without reducing package quality."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import unittest
from unittest import mock

from tests.built_maps import selected_sources

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('curated_compile', ROOT / 'tools/levels/compile.py')
COMPILE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPILE)


class CuratedCompileTests(unittest.TestCase):
    def test_default_acceptance_covers_every_lineup_source(self):
        with mock.patch.dict(os.environ):
            os.environ.pop('PLACES_PACKAGE_TEST_IDS', None)
            self.assertEqual(selected_sources(), [source for _, source in COMPILE.lineup()])
            self.assertEqual(len(selected_sources()), 8)

    def test_archive_shards_reject_unknown_or_duplicate_maps(self):
        for selected in ('', 'obsolete_dev_map', 'winter,winter'):
            with mock.patch.dict(os.environ, PLACES_PACKAGE_TEST_IDS=selected):
                with self.assertRaises(ValueError):
                    selected_sources()
        with mock.patch.dict(os.environ, PLACES_PACKAGE_TEST_IDS='winter'):
            self.assertEqual(selected_sources(), [ROOT / 'levels/winter.json'])

    def test_single_map_runs_normal_compile_and_all_quality_validation(self):
        manifest = {'variants': [{'lightmap_quality': quality}
                                 for quality in ('off', 'medium', 'full')]}
        with mock.patch('sys.argv', ['compile.py', '--map', 'winter']), \
             mock.patch.object(COMPILE.subprocess, 'run') as run, \
             mock.patch.object(COMPILE.subprocess, 'check_output', return_value=json.dumps(manifest)), \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(COMPILE.main(), 0)
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual([command[1] for command in commands], ['build', 'verify', 'validate'])
        self.assertIn(str(ROOT / 'levels/winter.json'), commands[0])
        self.assertNotIn('--variants', commands[0])
        self.assertIn('--require-current', commands[1])

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
