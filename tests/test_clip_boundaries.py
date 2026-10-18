"""Prevent one-shot false failures and declared-loop false approvals.

One tiny metadata fixture, no GLB decode or asset/map build; sub-millisecond CPU.
"""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools/entities'))
from check_clip_boundaries import clip_is_looped


class ClipBoundarySemantics(unittest.TestCase):
    def test_declared_one_shots_and_loops_override_legacy_names(self):
        document = {'asset': {'extras': {'places_entity_clips': {'clips': [
            {'name': 'door_open_push', 'loop': False},
            {'name': 'pounce', 'loop': True},
        ]}}}}
        self.assertFalse(clip_is_looped(document, 'DOOR_OPEN_PUSH'))
        self.assertTrue(clip_is_looped(document, 'pounce'))
        self.assertTrue(clip_is_looped(document, 'walk'))
        self.assertFalse(clip_is_looped({}, 'pounce'))
        self.assertTrue(clip_is_looped({}, 'idle'))
