"""Small deterministic authoring fixtures; no compiled maps or native window."""
import math
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools/levels'))
from railings import RailingAuthor, floor_height

CATALOG = {
    'outdoor:porch_railing_straight': {'size': [1.8, 1.05, .12]},
    'outdoor:porch_post': {'size': [.12, 1.05, .12]},
}


class RailingAuthorTests(unittest.TestCase):
    def level(self):
        return dict(rooms=[dict(x=-1, z=-1, width=7, depth=7)], props=[])

    def test_corner_and_independent_continuation_share_one_support(self):
        # Protects the doubled corner/end-post regression without loading any
        # production model. Six support props and five bounded sections.
        level = self.level()
        author = RailingAuthor(level, CATALOG)
        author.run('corner', [(0, 0, 0), (3.6, 0, 0), (3.6, 0, 3.6)])
        author.run('continuation', [(3.6, 0, 3.6), (5.4, 0, 3.6)])
        posts = [p for p in level['props'] if 'span' not in p]
        self.assertEqual(len(posts), 6)
        self.assertEqual(len({(p['x'], p['z']) for p in posts}), len(posts))
        for section in (p for p in level['props'] if 'span' in p):
            yaw = math.radians(section['rotation_degrees'])
            for direction in (-1, 1):
                distance = section['span']['length']/2*direction
                endpoint = (section['x']+math.cos(yaw)*distance,
                            section['z']-math.sin(yaw)*distance)
                support = min(posts, key=lambda p: math.hypot(p['x']-endpoint[0], p['z']-endpoint[1]))
                self.assertLess(math.dist(endpoint, (support['x'], support['z'])), .06*support['scale'])
                self.assertGreater(math.dist(endpoint, (support['x'], support['z'])), .04)

    def test_sloped_support_footprints_are_grounded_and_sections_keep_rise(self):
        # A single 0.4 m ramp catches midpoint-only floating post corners and
        # lost rise during module subdivision. Runs in milliseconds.
        level = self.level()
        level['ramps'] = [dict(x=0, z=0, width=4, depth=2, offset_y=0, rise=.4)]
        RailingAuthor(level, CATALOG).run('slope', [(.5, .05, 1), (3.5, .35, 1)])
        for prop in level['props']:
            base = floor_height(level, prop['x'], prop['z'])+prop['y']
            if 'span' in prop:
                self.assertAlmostEqual(prop['span']['rise']/prop['span']['length'], .1, places=5)
                continue
            half = .06*prop['scale']
            corners = [floor_height(level, prop['x']+offset, prop['z']) for offset in (-half, half)]
            self.assertLess(base, min(corners))
            self.assertLess(min(corners)-base, .008)
            self.assertGreater(base+1.05*prop['scale'], max(corners)+1.05)
