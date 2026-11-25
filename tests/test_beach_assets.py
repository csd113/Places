"""Beach rig interpolation and runtime-envelope regression checks."""
import copy
import math
import unittest

from assets.inspection.rig_pose import Model, _slerp, verify_report


class BeachTests(unittest.TestCase):
    def test_rotation_sweep_matches_spherical_runtime_interpolation(self):
        # At one quarter of a 120-degree turn, nlerp produces 27.8 degrees;
        # the renderer's SLERP produces 30. Bounds must sample the same pose.
        rotation = _slerp([0, 0, 0, 1], [0, math.sin(math.pi/3), 0, .5], .25)
        self.assertAlmostEqual(rotation[1], math.sin(math.pi/12), places=7)
        self.assertAlmostEqual(rotation[3], math.cos(math.pi/12), places=7)
        self.assertEqual(_slerp([0, 0, 0, 1], [0, 0, 0, -1], .5), [0, 0, 0, 1])

    def test_step_channel_holds_until_the_next_key(self):
        sample = Model.__new__(Model)._interpolate
        self.assertEqual(sample([0, 1], [[0], [4]], .999, False, interpolation="STEP"), [0])
        self.assertEqual(sample([0, 1], [[0], [4]], 1, False, interpolation="STEP"), [4])

    def test_horizontal_escape_fails_the_maintained_entity_gate(self):
        path = "assets/entities/beach_fish/model/beach_fish.glb"
        asset = dict(path=path, vertices=3, triangles=1, joints=1,
                     weight_problems=[], bind_min_m=[-1, 0, -1], bind_max_m=[1, 1, 1])
        clip = dict(asset=path, name="swim", duration=1, kind="swim", min_y_m=0,
                    max_edge_stretch=1, contact_frames=0,
                    posed_min_m=[-1, 0, -1], posed_max_m=[1, 1, 1])
        report = dict(assets=[asset], clips=[clip])
        self.assertEqual(verify_report(report), [])
        escaped = copy.deepcopy(report)
        escaped["clips"][0]["posed_max_m"][0] = 1.21
        self.assertTrue(any("axis 0" in failure for failure in verify_report(escaped)))

if __name__ == "__main__":
    unittest.main()
