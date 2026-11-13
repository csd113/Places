"""Tiny release-safety fixtures: no compilation, map bake, network or native app.

Protect the publication boundary: reject mismatched/corrupt build artifacts,
reject moved tags/published releases, and refuse clobbering existing assets.
These are otherwise untested upload decisions; fixtures total a few bytes and
run in milliseconds. One ZIP fixture checks preservation of executable modes.
"""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SPEC = importlib.util.spec_from_file_location('hosted', Path(__file__).resolve().parents[1] / 'tools/release/hosted.py')
hosted = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(hosted)


class HostedReleaseTests(unittest.TestCase):
    def test_artifacts_must_belong_to_same_run_and_source(self):
        info = dict(version='1.2.3', source_commit='a' * 40, repository='owner/repo', run_id='123')
        with tempfile.TemporaryDirectory() as scratch:
            folder = Path(scratch)
            for platform in hosted.PLATFORMS:
                path = folder / f'Places-1.2.3-{platform}.zip'
                path.write_bytes(b'fixture')
                hosted.write_json(folder / f'{platform}.json', dict(
                    **info, platform=platform, archive=path.name, bytes=7, sha256=hosted.digest(path)))
            self.assertEqual(len(hosted.verify_artifacts(folder, info)), 2)
            receipt = folder / 'Windows.json'
            original = json.loads(receipt.read_text())
            for key in ('source_commit', 'run_id', 'version', 'archive'):
                with self.subTest(key=key):
                    hosted.write_json(receipt, original | {key: 'wrong'})
                    with self.assertRaises(ValueError):
                        hosted.verify_artifacts(folder, info)
            hosted.write_json(receipt, original)
            (folder / original['archive']).write_bytes(b'corrupt')
            with self.assertRaises(ValueError):
                hosted.verify_artifacts(folder, info)

    def test_upload_refuses_conflicts_but_resumes_identical_assets(self):
        wanted = [dict(name='app.zip', sha256='a' * 64, bytes=7)]
        self.assertEqual(hosted.upload_plan(wanted, [dict(name='guide.pdf')]), ['app.zip'])
        existing = dict(name='app.zip', digest='sha256:' + 'a' * 64, size=7, state='uploaded')
        self.assertEqual(hosted.upload_plan(wanted, [existing]), [])
        for change in ({'digest': None}, {'digest': 'sha256:wrong'}, {'size': 9}, {'state': 'starter'}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                hosted.upload_plan(wanted, [existing | change])

    def test_destination_requires_exact_tag_commit_and_mutable_draft(self):
        info = dict(version='1.2.3', source_commit='a' * 40, repository='owner/repo')
        draft = dict(id=12, tag_name='v1.2.3', draft=True, immutable=False)
        with patch.dict(os.environ, GITHUB_REF='refs/tags/v1.2.3'):
            # A draft beyond page one must be found without the published-only
            # by-tag endpoint. This models the API visibility boundary cheaply.
            older = [dict(tag_name=f'v0.0.{index}') for index in range(100)]
            with patch.object(hosted, 'api', side_effect=[{'sha': 'a' * 40}, older, [draft]]) as api:
                self.assertEqual(hosted.destination(info), draft)
                self.assertEqual(api.call_args_list[-1].args[0],
                                 'repos/owner/repo/releases?per_page=100&page=2')
            for commit, releases in [('b' * 40, [draft]), ('a' * 40, [draft | {'draft': False}]),
                                     ('a' * 40, [draft | {'immutable': True}]),
                                     ('a' * 40, []), ('a' * 40, [draft, draft])]:
                with patch.object(hosted, 'api', side_effect=[{'sha': commit}, releases]), self.assertRaises(ValueError):
                    hosted.destination(info)
        with patch.dict(os.environ, GITHUB_REF='refs/heads/main'), patch.object(hosted, 'api') as api:
            with self.assertRaises(ValueError):
                hosted.destination(info)
            api.assert_not_called()

    @unittest.skipIf(os.name == 'nt', 'POSIX executable mode fixture')
    def test_archive_preserves_executable_mode_and_contents(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            stage = root / 'stage/Places'
            stage.mkdir(parents=True)
            binary = stage / 'places'
            binary.write_bytes(b'fixture')
            binary.chmod(0o755)
            archive = root / 'payload.zip'
            hosted.archive_payload(stage.parent, archive)
            with zipfile.ZipFile(archive) as result:
                self.assertEqual(result.read('Places/places'), b'fixture')
                self.assertEqual((result.getinfo('Places/places').external_attr >> 16) & 0o777, 0o755)


if __name__ == '__main__':
    unittest.main()
