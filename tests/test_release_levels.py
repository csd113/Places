"""Protect separate-release membership, complete lighting and preservation.

Tiny two-project/three-member ZIPs and mocked native validation keep these
regressions below one second; ordinary source tests never bake shipped maps.
"""
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('release_levels', ROOT / 'release/package-levels.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseLevelsTests(unittest.TestCase):
    def fixture(self, root, *, failed_full=False, wrong_version=False, wrong_weather=False, wrong_source=False,
                wrong_sky=False, wrong_ground=False):
        (root / 'release').mkdir()
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
            if project_id == 'story':
                maintained['package_version'] = '2.0.0'
            (project / 'places.project.json').write_text(json.dumps(maintained))
            name = project_id + '.placesmap'
            packaged = dict(maintained)
            packaged.setdefault('package_version', '1.0.0')
            if wrong_version:
                packaged['package_version'] = '3.0.0'
            index = dict(project=packaged, levels=[], sources={})
            with zipfile.ZipFile(maps / name, 'w') as archive:
                for identity in identities:
                    relative = identity + '.json'
                    authored = {'id': identity}
                    if identity == 'hub':
                        authored['sky'] = {'texture': 'outdoor:tex_sky_stars_01'}
                        authored['rooms'] = [{'material': 'core:asphalt'}]
                    if identity == 'home':
                        authored['weather'] = {'kind': 'snow', 'opacity': .1}
                        authored['weather_cycle'] = {'enabled': False, 'max_strength': .45}
                    bytes_ = json.dumps(authored).encode('utf-8')
                    (project / relative).write_bytes(bytes_)
                    recorded = json.dumps({'changed': identity}).encode('utf-8') if wrong_source else bytes_
                    blob = 'blobs/' + hashlib.sha256(recorded).hexdigest() + '.record'
                    archive.writestr(blob, recorded)
                    index['sources'][relative] = blob
                    semantics = json.loads(bytes_)
                    if identity == 'hub' and wrong_sky:
                        semantics['sky']['texture'] = 'beach:tex_sky_day_01'
                    if identity == 'hub' and wrong_ground:
                        semantics['rooms'][0]['material'] = 'winter:ice_01'
                    if identity == 'home' and wrong_weather:
                        semantics['weather']['opacity'] = .2
                    if identity == 'home':
                        semantics['weather']['count'] = 1400  # compiler-resolved default
                    semantics_name = identity + '.semantics.json'
                    archive.writestr(semantics_name, json.dumps(semantics))
                    manifest_name = identity + '.manifest.json'
                    manifest = dict(compiler_fingerprint='fixture', variants=[dict(lightmap_quality='off'),
                        dict(lightmap_quality='full', lightmap_failure='fallback' if failed_full else None,
                             entries=dict(lightmaps='atlas'))])
                    archive.writestr(manifest_name, json.dumps(manifest))
                    index['levels'].append(dict(id=identity, records={
                        'manifest.json': manifest_name, 'semantics.json': semantics_name}))
                archive.writestr('project.json', json.dumps(index))
            rows.append(dict(source='projects/' + project_id, package=name))
        (root / 'release/release-projects.json').write_text(json.dumps(dict(version='fixture', projects=rows, excluded=['annex'])))
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
            verified = [call.args[0] for call in native.call_args_list if call.args[0][1] == 'verify-project']
            self.assertEqual([Path(command[2]).name for command in verified], ['collection', 'story'])
            self.assertTrue(all('--require-current' in command and '--level' not in command for command in verified))

    def test_existing_archive_cannot_change_climate_ground_or_recoverable_source(self):
        # The compiler validates the internally consistent bytes, not whether
        # decoded weather or editable recovery still match the maintained source.
        # Witness each mismatch in the existing two-project, no-bake fixture.
        for patch_values, reason in [({'wrong_weather': True}, 'packaged weather differs'),
                                     ({'wrong_source': True}, 'recoverable source differs')]:
            with self.subTest(reason=reason), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                maps, compiler, assets = self.fixture(root, **patch_values)
                with patch.object(release, 'ROOT', root), patch.object(release.subprocess, 'run',
                        return_value=subprocess.CompletedProcess([], 0, '{"dependencies_intact":true}', '')):
                    with self.assertRaisesRegex(ValueError, reason):
                        release.collect(maps, compiler, assets)
                self.assertFalse(list(root.glob('*.zip')))

    def test_different_project_release_is_refused_before_output(self):
        # One tiny mocked package guards against normalizing away an explicit
        # release mismatch; no map bake or native process is added.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            maps, compiler, assets = self.fixture(root, wrong_version=True)
            with patch.object(release, 'ROOT', root), patch.object(release.subprocess, 'run',
                    return_value=subprocess.CompletedProcess([], 0, '{"dependencies_intact":true}', '')):
                with self.assertRaisesRegex(ValueError, 'packaged project differs'):
                    release.package_levels(maps, root / 'levels.zip', compiler, assets)
            self.assertFalse((root / 'levels.zip').exists())

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
