"""Coverage and camera-height regressions for the maintained lighting harness."""
import unittest
from tools.bench.capture_lighting_regions import views, inventory, require_matching_package, require_prepared_load


class LightingRegionCoverage(unittest.TestCase):
    def test_prepared_runtime_requires_positive_zero_bake_evidence(self):
        import json
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / 'runtime.jsonl'
            trace.write_text('')
            with self.assertRaisesRegex(RuntimeError, 'bake-free'):
                require_prepared_load(trace)
            for lighting_ms, atlas_ms in [(1, 0), (0, 1), (0, 0)]:
                trace.write_text(json.dumps(dict(event='preparation_result', detail=json.dumps(
                    dict(lighting_ms=lighting_ms, atlas_ms=atlas_ms)))) + '\n')
                if lighting_ms or atlas_ms:
                    with self.assertRaisesRegex(RuntimeError, 'bake-free'):
                        require_prepared_load(trace)
                else:
                    require_prepared_load(trace)

    def test_curved_partition_has_views_on_both_sides(self):
        data = {'rooms': [dict(x=0,z=0,width=8,depth=8,floor_y=-1)],
                'arc_walls': [dict(x=4,z=2,radius=2,thickness=.2,
                                   start_degrees=135,sweep_degrees=90)]}
        shots = {name: spawn for name,spawn,_ in views(data)}
        inner = list(map(float,shots['arc_walls_0_inner'].split(',')))
        outer = list(map(float,shots['arc_walls_0_outer'].split(',')))
        self.assertAlmostEqual(inner[2],3.3)
        self.assertAlmostEqual(outer[2],4.7)
        self.assertAlmostEqual(inner[1],.6)
        self.assertEqual(inner[3],180)
        self.assertEqual(outer[3],360)
        self.assertEqual(float(shots['arc_walls_0_inner_away'].split(',')[3]),360)
        self.assertEqual(float(shots['arc_walls_0_outer_away'].split(',')[3]),540)

    def test_rebaked_package_cannot_reuse_stale_capture_stage(self):
        from pathlib import Path
        from tempfile import TemporaryDirectory
        with TemporaryDirectory() as directory:
            staged = Path(directory) / 'staged.placesmap'
            requested = Path(directory) / 'rebuilt.placesmap'
            requested.write_bytes(b'new package')
            require_matching_package(staged, requested)
            staged.write_bytes(b'old package')
            with self.assertRaisesRegex(RuntimeError, 'differs from requested rebuild'):
                require_matching_package(staged, requested)
            staged.write_bytes(requested.read_bytes())
            require_matching_package(staged, requested)

    def test_room_cameras_cover_all_cardinals_at_explicit_eye_height(self):
        data = {'rooms': [dict(x=10, z=20, width=4, depth=6, floor_y=-2, height=3)]}
        shots = views(data)
        self.assertEqual(len(shots), 6)
        self.assertEqual({camera for _, _, camera in shots}, {'0,-8','90,-8','180,-8','270,-8','0,-70','0,70'})
        for _, spawn, _ in shots:
            x, y, z, _ = map(float, spawn.split(','))
            self.assertEqual((x,z), (12,23))
            self.assertAlmostEqual(y, -.4)

    def test_tall_room_and_raised_region_have_distinct_views(self):
        data = {'rooms':[dict(x=0,z=0,width=24,depth=12,height=17,floor_y=-1)],
                'floor_regions':[dict(x=2,z=2,width=4,depth=4,offset_y=3)]}
        shots = views(data)
        self.assertEqual(len(shots), 12)
        self.assertEqual(sum(name.endswith('ceiling') for name,_,_ in shots), 2)
        raised = next(spawn for name,spawn,_ in shots if name=='floor_regions_0')
        self.assertEqual(tuple(map(float,raised.split(',')))[:3], (4,3.6,4))

    def test_every_maintained_room_has_a_camera(self):
        import json
        from pathlib import Path
        for entry in inventory():
            if not entry['playable']:
                continue
            level=json.loads(Path(entry['source']).read_text())
            names=[name for name,_,_ in views(level)]
            for room in range(len(level['rooms'])):
                self.assertTrue(any(name.startswith(f'room_{room}_') for name in names),entry['id'])


if __name__ == '__main__':
    unittest.main()
