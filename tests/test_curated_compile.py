"""Opt-in package acceptance must select only maintained distributions."""
import os
import unittest
from unittest import mock

from release.curated_maps import lineup, source_levels
from tests.built_maps import selected_distributions, selected_sources


class CuratedSourceSelectionTests(unittest.TestCase):
    def test_default_acceptance_covers_every_lineup_source(self):
        with mock.patch.dict(os.environ):
            os.environ.pop('PLACES_PACKAGE_TEST_IDS', None)
            expected = [path for _, source in lineup() for _, path in source_levels(source)]
            self.assertEqual(selected_sources(), expected)
            self.assertTrue(expected)

    def test_archive_shards_reject_unknown_or_duplicate_distributions(self):
        identity, source = lineup()[0]
        for selected in ('', 'obsolete_dev_map', f'{identity},{identity}'):
            with mock.patch.dict(os.environ, PLACES_PACKAGE_TEST_IDS=selected):
                with self.assertRaises(ValueError):
                    selected_sources()
        with mock.patch.dict(os.environ, PLACES_PACKAGE_TEST_IDS=identity):
            self.assertEqual(selected_distributions(), [(identity, source)])
            self.assertEqual(selected_sources(), [path for _, path in source_levels(source)])


if __name__ == '__main__':
    unittest.main()
