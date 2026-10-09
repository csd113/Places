# Recovered review source and gameplay audit

2026-10-08 UTC. [Exact machine receipt](integration-recovered-review-audit.json).
The read-only audit compares preserved entry packages to the primary's completed
first C1 builds. Those builds passed source preparation, required-current and
full package decoding; their results are preserved as superseded after the
bounded roof correction below. No Cargo, compiler, bake or native command was
run by this specialist. Later source changes were separately allocated by the
primary after it stopped all active build/native jobs.

The audit verifies each role's declared length and SHA, decodes bounded PLCL v2
little-endian records against `src/package/collision.rs` / `src/level.rs`, checks
all finite values/counts/trailing bytes, and compares exact bytes and f32 fields.
No epsilon-equivalence or raw-byte assertion waiver is used. The preserved
packages are under `debug-maps/art-style-hero/evidence/stage7-entry/maps/levels/`.
Recovered JSON files remain byte-identical to their original `semantics` records.

## Blizzard review: a real conservative-bound defect

Both collision records have 264 boxes and 9,428 bytes, PLCL v2. Semantics is
exactly 143,251 bytes, SHA `d8c0196ec161e94c5116ee50ec81e670571788f1af1e0a43666618120955422a`.
Navigation is exactly 840,286 bytes, SHA
`43c0f461f23983bcb0875aa4c80b2c04a8f99b51fddbb69581e0fbb9833d1f08`.
The collision SHA changes from
`0713eb740bffc2837134ab1f5c8568777f1a3868b17368aeff9da10b2ea4c929` to
`3ffa860a9e86673abda09e554a2c8f5465e55775642fd78bfc243393ab6329fc`.

Exactly 262 box tuples remain identical. Records 28 and 30, corresponding to
the pre-ridge halves of authored walls 6 and 7, change only `max_y`:

| Field | Preserved original | First C1 rebuild |
| --- | ---: | ---: |
| Top height, metres | 4.300000190734863 | 4.299999713897705 |
| Exact f32 bits | `0x4089999a` | `0x40899999` |
| Height removed | | `1/2097152` metre |

The two removed top slabs have strict positive volume
`448600916099/1152921504606846976` and
`112149872509/288230376151711744` cubic metres. Neither intersects any rebuilt
box with positive volume. Their union is therefore genuinely absent; this is
not partition-only serialization. The 2,026-byte floor/ceiling/water/ladder tail
is byte-identical, SHA
`00b15e7a35f6690ee38fa90199d926b4c8cb429905f92bc54bffd8319232de45`:
44 floor rooms, five ramps, one staircase, 17 regions, three gable ceilings,
zero water volumes and zero ladders. Step classifications are unchanged:
223 ordinary boxes and 41 step-permitting rims.

`src/level.rs::solid_wall_slices` previously recovered a span's endpoint maximum
from two interior samples to avoid borrowing an adjacent room's roof. For this
gable, span `[0, 2.719999313354492]` samples heights 3.212000608444214 and
3.937333345413208. Exact f32/FMA reproduction gives 4.299999713897705, one ULP
below the real owned ridge 4.300000190734863. The source/codec did not change;
the historical geometry algorithm and the current extrapolation differ.

An exact predicate counterexample also exists: foot Y 4.298999786376953 plus
the existing `STEP_EPS` computes 4.299999713897705. `WallAabb::blocks_body` is
true for the original top and false for C1's lower top. The highest-support
query at `(8.15, -28)` returns the differing top. This is a predicate/support
counterexample, not a measured native route or claim that the state is reached
in ordinary play. Identical navigation and interactive semantics cannot justify
global traversal equality in the presence of that witness.

## Snowfall contrast: explicit legacy weather defaults

Collision is byte-identical: PLCL v2, 86 bytes, one box, SHA
`ab2618d6f51d276779555d670e23c3d1a31ed37c84cc0255c08c088869953433`.
It contains one floor room, no ramps/stairs/regions/closed ceilings/water/ladders.
Navigation is byte-identical, 384,939 bytes, SHA
`bddd2f070d217cd4ad7cb7eff266ed29e55f8364e39f6e817fde9cdcc1f033af`.

Raw semantics changes from 1,814 to 1,941 bytes. Every JSON difference is one
of four previously omitted `SnowfallDef` defaults now serialized explicitly:
intensity 1, storm severity 0, visibility 5 m and fog colour `[0.68, 0.73, 0.79]`
in their exact f32 representation. `src/weather.rs` declares these defaults
under `#[serde(default)]`; all other semantic paths are equal. Applying exactly
those defaults makes the semantic JSON objects equal, and that normalization
is idempotent. Raw bytes are still reported unequal. No compiler repeat-run
idempotence claim is inferred from the JSON normalization alone.

## Authorized correction and remaining acceptance

The primary separately authorized a bounded root fix. `LevelSurfaces` now chooses
each cut span's midpoint roof owner and evaluates both exact endpoints against
that same owner. The maximum stays exact at a ridge and cannot borrow the
neighbour's taller/shorter endpoint. Collision, geometry/coalescing, buried-wall
validation and geometry estimates share this owned-span decomposition. The
generic public closure API retains its existing owner-free fallback. No broad
tolerance, inflated box, camera exception or source-content edit is added.

Geometry revision 7 invalidates prepared products. Source changes are frozen in
`src/level.rs`, `src/level/tests/wall_ceiling.rs`, `src/loader.rs`,
`src/render/common/mod.rs` and `src/render/common/architecture.rs`.
The existing face emitter already samples endpoints against the fixed span owner
and needs no separate `geometry.rs` edit. Four focused regressions require exact
gable/ridge bounds, adjacent-roof endpoint maxima, flat/explicit bounds and the
body/support counterexample. The existing adjacent-roof isolation test now uses
the actual engine helper; generic API tests remain intact.

The primary owns format, focused/full Rust gates, strict debug/release Clippy,
new normal compiler identity and all final rebakes. A final recovered Blizzard
comparison must verify the entire record/occupied union independently; source
implementation alone is not evidence that the output now equals the original.
The C1 measurements above remain unchanged and superseded. No final gameplay
preservation claim is made before that fresh comparison.

The completed C1 archives are now byte-exact in the
[interrupted campaign preservation receipt](c1-interrupted-preservation.json),
including both review packages. Historical command targets may subsequently hold
geometry-7 products; use these preserved C1 paths for the counterexample. This
partial campaign is superseded and does not establish final acceptance.
