# Curved architecture modules

`curved_architecture.json` contains **alternatives**, not a level: copy one entry into the level collection and set its circle-centre `x`, `z` and base `y`. The 90° and 180° walls deliberately share the same local origin; do not place both there. These are the engine-native modular geometry, with collision and baked-light occlusion, rather than decorative GLB substitutes.

- Walls: 2 m centreline radius, 24 cm thickness, 2.8 m height. Twelve facets per quarter turn keep maximum chord deviation under 5 mm. Both radial ends are closed. Join matching radii/thickness at exact compass angles.
- Pillar: 60 cm diameter, 2.8 m height, 16 facets; circle-centre floor pivot and under 6 mm chord deviation.
- All dimensions are metres, +Y up; wall yaw starts at north and increases clockwise. Materials tile in world metres independently on inner/outer walls. Swap the existing material ID as required.
- `tests/fixtures/levels/asset_polish_showcase.json` separates the modules in a complete loadable QA level.
