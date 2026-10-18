"""Explicit compiled-distribution acceptance; first run sh tools/build-maps.sh."""
import hashlib
import json
import os
from pathlib import Path
import unittest
import zipfile

from tools.levels.curated_maps import level_views, lineup, project_manifest, source_levels

ROOT = Path(__file__).resolve().parents[1]


def selected_distributions():
    """Explicit package selections retain every sub-map's assertions."""
    rows = lineup()
    identities = [identity for identity, _ in rows]
    selected = os.environ.get('PLACES_PACKAGE_TEST_IDS')
    requested = identities if selected is None else selected.split(',')
    if not requested or len(requested) != len(set(requested)) or any(
            identity not in identities for identity in requested):
        raise ValueError('PLACES_PACKAGE_TEST_IDS must name unique curated distribution IDs')
    by_id = dict(rows)
    return [(identity, by_id[identity]) for identity in requested]


def selected_sources():
    """Every declared playable source, including all project sub-maps."""
    return [path for _, source in selected_distributions() for _, path in source_levels(source)]


def package_files():
    return sorted(Path(os.environ.get('PLACES_PACKAGE_TEST_ROOT', ROOT / 'assets/levels')).glob('*.placesmap'))


def package_documents(path):
    """Read semantics through each project's virtual record view."""
    with zipfile.ZipFile(path) as archive:
        for identity, records in level_views(archive):
            yield (identity, json.loads(archive.read(records['manifest.json'])),
                   json.loads(archive.read(records['semantics.json'])))


def record_identity(archive, name, verified):
    """Hash shared records once, streaming large prepared atlases."""
    if name not in verified:
        digest, size = hashlib.sha256(), 0
        with archive.open(name) as stream:
            while block := stream.read(1024 * 1024):
                digest.update(block)
                size += len(block)
        verified[name] = digest.hexdigest(), size
    return verified[name]


class BuiltMapTests(unittest.TestCase):
    def test_the_bundled_packages_are_present_valid_and_content_addressed(self):
        packages = {path.stem: path for path in package_files()}
        sources = dict(selected_distributions())
        self.assertEqual(set(packages), set(sources), 'each curated distribution has exactly one package')
        for distribution_id, path in packages.items():
            with self.subTest(distribution=distribution_id), zipfile.ZipFile(path) as archive:
                names = archive.namelist()
                verified = {}
                views = level_views(archive)
                expected = source_levels(sources[distribution_id])
                self.assertEqual([identity for identity, _ in views], [identity for identity, _ in expected])
                if project_manifest(sources[distribution_id]) is not None:
                    index = json.loads(archive.read('project.json'))
                    self.assertEqual(index['project'], project_manifest(sources[distribution_id]))
                    expected_names = {'project.json'}
                    expected_names.update(index['sources'].values())
                    for _, records in views:
                        expected_names.update(records.values())
                    self.assertEqual(set(names), expected_names, 'no undeclared bundle records')
                    for name in names:
                        if name != 'project.json':
                            self.assertEqual(name.split('/', 1)[1].split('.', 1)[0],
                                             record_identity(archive, name, verified)[0], name)
                    for relative, record in index['sources'].items():
                        self.assertEqual(archive.read(record),
                                         (sources[distribution_id].parent / relative).read_bytes(), relative)
                for identity, records in views:
                    self.check_level(archive, identity, records, verified)

    def check_level(self, archive, identity, records, verified):
        manifest = json.loads(archive.read(records['manifest.json']))
        self.assertEqual(manifest['package_format'], 1, identity)
        self.assertEqual(manifest['id'], identity)
        self.assertTrue(manifest['dependencies'], f'{identity} declares dependencies')
        self.assertEqual({v['lightmap_quality'] for v in manifest['variants']}, {'off', 'full'}, identity)
        semantics = json.loads(archive.read(records['semantics.json']))
        self.assertEqual(semantics['id'], identity)
        self.assertEqual(semantics['format_version'], 1, identity)
        declared = {entry['name'] for entry in manifest['entries']}
        self.assertEqual(set(records), {'manifest.json', *declared})
        for entry in manifest['entries']:
            digest, size = record_identity(archive, records[entry['name']], verified)
            self.assertEqual(digest, entry['sha256'], entry['name'])
            self.assertEqual(size, entry['bytes'], entry['name'])
            if entry['name'].startswith('blobs/'):
                self.assertEqual(entry['name'].split('/', 1)[1].split('.', 1)[0], digest)
        # Every sub-map carries prepared worlds; the player never compiles.
        for variant in manifest['variants']:
            for role in ('mesh', 'props', 'lighting', 'collision', 'navigation'):
                self.assertIn(variant['entries'][role], records, f'{identity} {role}')
            if variant['lightmap_quality'] != 'off':
                self.assertFalse(variant.get('lightmap_failure'), identity)
                self.assertIn(variant['entries']['lightmaps'], records, identity)
            if 'probes-rgba8' in manifest['required_capabilities']:
                self.assertTrue(variant['entries']['probes'], f'{identity} probe payload')
            for payload in variant['entries']['probes']:
                self.assertEqual(payload['count'], len(payload['cubemaps']), identity)
                for cubemap in payload['cubemaps']:
                    self.assertIn(cubemap, records, identity)
                    self.assertEqual(cubemap.split('/', 1)[1].split('.', 1)[0],
                                     record_identity(archive, records[cubemap], verified)[0], cubemap)
