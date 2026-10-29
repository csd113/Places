"""Protect separate-release membership, complete lighting and preservation.

Tiny two-project/three-member ZIPs and mocked native validation keep these
regressions below one second; ordinary source tests never bake shipped maps.
"""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('release_levels', ROOT / 'tools/package-levels.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseLevelsTests(unittest.TestCase):
    def fixture(self, root, *, failed_full=False):
        (root / 'tools').mkdir()
        maps = root / 'maps'
        maps.mkdir()
        assets = root / 'assets'
        assets.mkdir()
        (assets / 'catalog.json').write_text('{}')
        compiler = root / 'compiler'
        compiler.write_bytes(b'compiler identity')
        rows = []
        for project_id, identities in [('collection', ['hub', 'home']), ('story', ['chapter'])]:
            project = root / 'projects' / project_id
            project.mkdir(parents=True)
            maintained = dict(id=project_id, levels=[dict(id=identity, source=identity + '.json') for identity in identities])
            (project / 'places.project.json').write_text(json.dumps(maintained))
            name = project_id + '.placesmap'
            index = dict(project=maintained, levels=[])
            with zipfile.ZipFile(maps / name, 'w') as archive:
                for identity in identities:
                    (project / (identity + '.json')).write_text('{}')
                    manifest_name = identity + '.manifest.json'
                    manifest = dict(compiler_fingerprint='fixture', variants=[dict(lightmap_quality='off'),
                        dict(lightmap_quality='full', lightmap_failure='fallback' if failed_full else None,
                             entries=dict(lightmaps='atlas'))])
                    archive.writestr(manifest_name, json.dumps(manifest))
                    index['levels'].append(dict(id=identity, records={'manifest.json': manifest_name}))
                archive.writestr('project.json', json.dumps(index))
            rows.append(dict(source='projects/' + project_id, package=name))
        (root / 'tools/release-projects.json').write_text(json.dumps(dict(version='fixture', projects=rows, excluded=['annex'])))
        return maps, compiler, assets

    def test_separate_zip_retains_every_collection_member_and_exact_package_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            maps, compiler, assets = self.fixture(root)
            with patch.object(release, 'ROOT', root), patch.object(release.subprocess, 'run',
                    return_value=subprocess.CompletedProcess([], 0, '{"dependencies_intact":true}', '')) as native, \
                    patch.object(release.subprocess, 'check_output', return_value='source-sha\n'):
                output = root / 'levels.zip'
                result = release.package_levels(maps, output, compiler, assets)
            self.assertEqual((result['member_count'], result['variant_count']), (3, 6))
            with zipfile.ZipFile(output) as archive:
                self.assertEqual(set(archive.namelist()), {'Maps/collection.placesmap', 'Maps/story.placesmap', 'LEVELS.json', 'README.txt'})
                for package in maps.iterdir():
                    self.assertEqual(archive.read('Maps/' + package.name), package.read_bytes())
            verified = [call.args[0][call.args[0].index('--level') + 1] for call in native.call_args_list if '--level' in call.args[0]]
            self.assertEqual(verified, ['hub', 'home', 'chapter'])

    def test_extra_missing_or_fallback_content_and_existing_output_preserve_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            maps, compiler, assets = self.fixture(root, failed_full=True)
            sentinel = root / 'keep.zip'
            sentinel.write_bytes(b'unique previous output')
            with patch.object(release, 'ROOT', root), patch.object(release.subprocess, 'run',
                    return_value=subprocess.CompletedProcess([], 0, '{"dependencies_intact":true}', '')):
                with self.assertRaisesRegex(ValueError, 'already exists'):
                    release.package_levels(maps, sentinel, compiler, assets)
                extra = maps / 'annex.placesmap'
                extra.write_bytes(b'unselected source stays')
                with self.assertRaisesRegex(ValueError, 'differs from release inventory'):
                    release.package_levels(maps, root / 'extra.zip', compiler, assets)
                self.assertEqual(extra.read_bytes(), b'unselected source stays')
                extra.unlink()
                with self.assertRaisesRegex(ValueError, 'intact Static and Full'):
                    release.package_levels(maps, root / 'fallback.zip', compiler, assets)
                (maps / 'story.placesmap').unlink()
                with self.assertRaisesRegex(ValueError, 'differs from release inventory'):
                    release.package_levels(maps, root / 'missing.zip', compiler, assets)
            self.assertEqual(sentinel.read_bytes(), b'unique previous output')
            self.assertEqual({p.name for p in root.glob('*.zip')}, {'keep.zip'})


if __name__ == '__main__':
    unittest.main()
