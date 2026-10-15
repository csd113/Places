"""Measured light anchors for the completed modular Winter string kit.

The retained geometry helpers describe the historical procedural source only;
completed GLBs are imported artwork and are never regenerated here. Nothing in
this module runs in the player or paints texture imagery.
"""

# Clear attachment-centre span, sag and historical bulb count. Keep the latter
# as the reference for each placed span's total authored illumination.
SPANS = {
    'short': (2.8, .15, 3),
    'medium': (4.0, .28, 5),
    'long': (6.6, .40, 7),
}
CABLE_RADIUS = .006
BULB_HEIGHT = .14
DROP = .08

# Local (x, y) point sources, measured 2 cm below the completed glass bottoms.
# A point inside the opaque glass would shadow itself in prepared transport.
EMITTER_ANCHORS = {
    'short': ((-1.323442, .159063), (-.661721, .024766), (0, -.02),
              (.661721, .024766), (1.323442, .159063)),
    'medium': ((-1.923071, .292095), (-1.282047, .118709), (-.641024, .014677),
               (0, -.02), (.641024, .014677), (1.282047, .118709),
               (1.923071, .292095)),
    'long': ((-3.223310, .421780), (-2.578648, .262740), (-1.933986, .139041),
             (-1.289324, .050685), (-.644662, -.002329), (0, -.02),
             (.644662, -.002329), (1.289324, .050685), (1.933986, .139041),
             (2.578648, .262740), (3.223310, .421780)),
}

# Centre of the delivered lantern's front aperture, clear of its base frame.
TIMBER_LANTERN_SOURCE = (0, 2.56, .167062)


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
    anchors = EMITTER_ANCHORS[variant]
    source_intensity = intensity * SPANS[variant][2] / len(anchors)
    return [{'shape': 'point', 'offset': [x, y, 0],
             'intensity': source_intensity, 'range': radius, 'color': [1, .60, .20],
             'falloff': 'smooth'} for x, y in anchors]
