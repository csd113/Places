"""Prevent one-shot false failures and declared-loop false approvals.

One metadata case and direct reads of shipped rigs; no renderer or map build.
"""
from pathlib import Path
import unittest

from assets.inspection.animation import (
    NON_BEACH_CLIP_MODELS, SPOONER_TRANSITIONS, clip_is_looped, validate_clip_boundaries,
)
from assets.inspection.rig_pose import Model


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

class ShippedClipBoundariesTests(unittest.TestCase):
    def test_current_non_beach_models_keep_loop_and_transition_endpoints(self):
        # The synthetic marker test cannot catch a mismatched real GLB key,
        # rest transform or skin. Read only the shipped rigs; no renderer.
        root = Path(__file__).resolve().parents[1] / "assets/entities"
        for name in NON_BEACH_CLIP_MODELS:
            with self.subTest(model=name):
                model = Model(root / name / "model" / (name + ".glb"))
                transitions = SPOONER_TRANSITIONS if name == "spooner-man" else ()
                self.assertGreater(validate_clip_boundaries(model, name, transitions), 0)


if __name__ == '__main__':
    unittest.main()
