"""Shared offline geometry/light anchors for the modular winter string kit.

The map author and prop builder use these same metre-space bulb positions.
Nothing in this module runs in the player or paints texture imagery.
"""

# Clear attachment-centre span, sag, bulb count. Each span fits the existing
# eight-lights-per-prop contract without grouped sources behind opaque walls.
SPANS = {
    'short': (2.8, .15, 3),
    'medium': (4.0, .28, 5),
    'long': (6.6, .40, 7),
}
CABLE_RADIUS = .006
BULB_HEIGHT = .14
DROP = .08


def cable_height(variant, x):
    length, sag, _ = SPANS[variant]
    return sag * (2 * x / length) ** 2 + BULB_HEIGHT + DROP


def bulbs(variant):
    length, _, count = SPANS[variant]
    return [(length * ((index + 1) / (count + 1) - .5),
             cable_height(variant, length * ((index + 1) / (count + 1) - .5)) - DROP)
            for index in range(count)]


def attachment_height(variant):
    length, _, _ = SPANS[variant]
    return cable_height(variant, length / 2)


def size(variant):
    length, _, _ = SPANS[variant]
    return [length + .05, attachment_height(variant) + .025, .09]


def lights(variant, intensity=.32, radius=4.5):
    # Keep the source 2 cm below the glass. A point inside opaque bulb geometry
    # would shadow itself in the prepared transport solve.
    return [{'shape': 'point', 'offset': [round(x, 6), round(top-BULB_HEIGHT-.02, 6), 0],
             'intensity': intensity, 'range': radius, 'color': [1, .60, .20],
             'falloff': 'smooth'} for x, top in bulbs(variant)]
