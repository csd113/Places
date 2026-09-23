# Renderer-change validation

How each renderer change was checked, and what the pixel comparison
actually shows. The point of recording this is that "0 pixels differ" is only a
meaningful claim if the comparison holds everything else fixed.

## Same-build A/B: every optimisation is pixel-exact

Each optimisation has a benchmark switch that changes exactly one submission
decision. Comparing the two images produced by the *same* release binary with and
without the switch isolates that optimisation from every other variable,
including compiler code generation:

| Comparison (same binary) | Differing pixels |
|---|---:|
| indexed vs flat (`LIMINAL_BENCH_NOINDEX`) | **0** |
| packed 24 B vs exact 36 B (`LIMINAL_BENCH_EXACT_VERTEX`) | **0** |
| culling on vs off (`LIMINAL_BENCH_NOCULL`) at a fixed grid | **0** |

Measured on levels since retired (the 400-chair stress level, Level 1 and the
Asset Demo) plus the `prop_showcase`, `prop_stress` and `test_room` fixtures,
both camera-away and camera-facing. So spatial culling, indexed submission and
the packed vertex layout each change the rendered image by nothing at all.

## Against the pre-optimisation binary

`visual_check.py --baseline target/agent-work/baseline/liminal-rust` reports, per shot,
the number of differing pixels and the worst channel delta, and fails only above
a tolerance (default 24 of 255).

For the chair stress levels it reports ~29 % of pixels differing with a worst
delta of 65. That number is the same for *all four* submission variants of the
current build (`packed`, `EXACT_VERTEX`, `NOINDEX`, `NOCULL`), which is what
identifies its cause: it is not any of the optimisations, it is that the current
binary was **recompiled from restructured source**.

Two distinct effects add up to it:

1. **±1 code value over most of the image.** The frame is multiplied by a
   texture and written to an 8-bit framebuffer, so one ULP of difference in a
   transformed vertex position propagates to one unit in the last bit of the
   output pixel. Every variant shows exactly the same ±1 pattern.
2. **One scanline of edge flips.** 67 pixels on rows 283–284 of a 544-row frame
   change by up to 65 because a near-degenerate horizontal silhouette edge (the
   row of distant chair tops, all at the same height with a level camera) sits
   within an ULP of a pixel boundary and rounds to the other side. This is a
   sub-pixel difference, not a missing or extra triangle: the two images draw
   the same triangle set, which the tests below assert directly.

## Structural (non-pixel) checks

Pixel comparison alone cannot say "the same triangles were drawn". These
invariants are asserted in the test suite instead:

* every range's indices address only its own vertex block, and every vertex in
  the block is referenced by at least one index;
* a range with *n* quads indexes at most *4n* distinct vertices and at least
  *2n*;
* every range's AABB contains all of its own vertices, and is finite and
  non-empty;
* indexing never merges two vertices that differ in any attribute bit — tested
  for position, UV, colour (the baked light) and alpha;
* the triangle winding of an indexed quad matches the pre-indexing emitter
  order `(p0,p1,p2), (p0,p2,p3)`;
* the geometry a level builds is identical whichever cell size is used, as a
  multiset of drawn vertices — only the submission order changes.

## Grid resolution

Changing `LIMINAL_CELL_METRES` changes the *order* in which identical triangles
are submitted (ranges are emitted cell-major). On a level with deliberately
self-intersecting geometry that resolves a handful of coincident-depth pixels
differently: 12 m versus 96 m on the chair stress level differs in 676 of
522,240 pixels, in components of at most 98 pixels, worst delta 67. The shipping
configuration uses the adaptive grid, at which this comparison against the
pre-optimisation build is the 67-pixel scanline described above.

## HUD and menus

The main menu was captured from both builds and compared the same way. The HUD
pipeline (font atlas, blend, orthographic projection, packed alpha) is
unchanged: outside the two expected effects below, the menu is pixel-identical.

* The version string in the menu reads `v0.7.0` in the baseline build and
  `v0.8.0` here, which is a real difference in the image and is *supposed* to be
  there. Any menu capture comparison across a version bump must expect it.
* 96 pixels differ by more than the tolerance, all of them edge pixels of that
  version digit, plus the usual ±1 code value elsewhere.
