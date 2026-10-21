# Office demo validation

`Office: After Hours` is a playable three-floor environment discovered as
`office_demo`. A believable office upstairs gives way to offset corridors and
displaced furniture, then stained basement halls with dark gaps between lights.
Two ramps and shared landings physically connect every floor. Existing Office
concept art, materials and modular furniture supply the visual language.

The source is [projects/environments/levels/office.json](../../projects/environments/levels/office.json), maintained
by [build_office.py](../../tools/levels/build_office.py). The normal compiler
creates the ignored `assets/levels/office_demo.placesmap`; the curated lineup
and generated model zoo register the map and new cockroach through existing
external discovery. Other authored maps retain their layouts and content.

```sh
RUSTC_WRAPPER= cargo build --release --bins
python3 tools/levels/build_office.py --check
sh tools/build-maps.sh --map office_demo --workers 8
PLACES_LEVEL=office_demo target/release/places
```

The main floors sit at 0, 3.6 and 7.2 m, with 2.8 m clear height. The 3 m wide
ramps rise 3.6 m over 10 m and have guardrails. Middle-floor furniture burial
is intentional and confined to wall-side pockets. Explicit trim avoids
stacked-floor ownership ambiguity and crossing-wall overlaps.

## Basement encounters and lighting

One concrete mannequin wanders the reachable basement, resting with its arms
down and running with its arms forward. The added `run_arms_forward` clip
combines the existing running legs and forward-arm pose; the original geometry,
skin, textures and five clips remain intact. Its navigation radius covers the
running arm envelope. It has no pursuit or combat behavior.

Two small cockroaches wander slowly in dark basement pockets. Player sight
within 2.1 m triggers a scurry toward the shortest complete reachable route
among three authored cabinet refuges. They physically enter the refuge, hide
for 15 or 19 seconds, return along a path, then observe a cooldown. Raised
cabinet plinths provide real clearance. Unreachable or newly blocked paths
cannot make an insect disappear. The new reusable model has 480 triangles,
25 joints and idle/walk/run clips; its opaque texture is a committed PNG.
See the [cockroach](../assets/ENTITIES.md#cockroach) and
[mannequin](../assets/ENTITIES.md#concrete-mannequin) maintenance notes.

Generic AI additions provide bounded routes, authored shelters and named
locomotion clips. The current compiled navigation grid represents one surface
per X/Z cell: the basement owns that surface, and all three actors have a
basement height band and inset home bounds. Player collision and support handle
the connected upper floors independently. There is no cross-floor NPC travel.

Two fluorescents have independent, sparse failure sequences. Full lighting
changes the luminous face, prepared illumination group and model lighting
together. Static cannot animate its baked field, so these two fixtures opt into
`hold_in_static`, retaining their baked-on face and illumination. Other fixtures
keep their previous behavior. Lighting remains Static/Full, independently of
texture quality and filtering; the UV material system is unchanged.

## Validation on 12 October 2026

Actual native wgpu/Metal runs on an Apple M2 Pro used a 1600 × 900 drawable,
65-degree FOV, bloom enabled and reflections off. Three composition passes
preceded final captures. Held-control walks traversed every ramp in both
directions, the floor entrances and basement passages. A continuous 47.6-second
walk started at the authored top spawn and reached the basement without a floor
leak. Wall collision stopped the player at the expected body clearance.

Native state traces and images confirmed basement-only mannequin movement,
arms-down idle, arms-forward running, physical roach refuge arrival, hidden
holds and visible return. The measured mannequin routes had no stuck or
unplaced state. Models retained environmental shading in lit and dark areas.
Full fixture failures visibly removed their light pools. Corresponding Static
on/off captures had identical bulb, wall and carpet pixels. Full/Low textures,
Static/High textures and a live independent filtering/lighting cycle passed.

Strict geometry reported **0 errors and 0 warnings**. Two guarded bridge-edge
openings have narrowly scoped authoring annotations. Archive, source freshness,
navigation records and both lighting variants validated through the normal
compiler. The package is **24,852,778 bytes**, SHA-256
`4effb221c87de00ad2a300bc5a5e1be728b2ee0d9dda80f9aeef784ee1374cf0`.
Its final display-name refresh reused verified prepared geometry and lighting.

Local checks passed:

- `cargo fmt --all --check`, strict workspace Clippy in debug and release with
  all targets/features and `-D warnings`, and workspace tests with all features.
  The library suite passed 2,278 tests; 19 existing developer checks remained
  ignored. Binary and integration tests also passed.
- Python source contracts passed 351 tests with four existing native smoke
  skips. Asset validation covered 555 assets and 336 placeables with no warnings;
  the full model/texture audit found no integrity errors.
- Office/model-zoo generation, cockroach regeneration and preserving mannequin
  clip regeneration checks passed. Hosted CI checks source and bounded fixtures;
  it does not bake production maps.

## Bounded performance and storage

The full curated-lineup refresh exposed a pre-existing Model Zoo capacity
blocker. The exact zoo source from handover commit `79c78df` also overflows the
unchanged 12-page Full budget (29,783 charts placed before fallback); the new
registered insect is not its cause. A bounded ownership-cell experiment was
insufficient and was reverted. Zoo content and renderer limits are preserved.
Its fallback archive is not a passing Full export; the later map certification
work must address that capacity. Office's own Full package passes this gate.

Three final Full/High runs each measured 300 frames after 60 warmup frames, with
GPU completion enabled, vsync disabled and no capture readback or navigation
logging. Every frame presented successfully. These are bounded desktop samples,
not a sustained frame-rate guarantee.

| View | Mean frame ms | Median ms | 95th percentile ms | Mean update ms |
| --- | ---: | ---: | ---: | ---: |
| Top | 7.80 | 8.08 | 12.93 | 1.79 |
| Middle | 7.45 | 8.03 | 16.46 | 1.50 |
| Basement | 9.88 | 8.13 | 16.48 | 1.97 |

The three actors use two meshes, three character draws and 5,849 vertices.
Combined update timing includes AI, skinning, input and other gameplay work;
isolated NPC CPU cost was not measured. The Full atlas uses four pages and two
switchable groups, occupying 192 MiB on the GPU; world textures occupy about
41.3 MiB. The final physical bake took 282 seconds with eight workers. Curated
screenshots total 6,727,200 bytes. Reproducible diagnostics, temporary captures
and build output are disposable; source assets and the normal local packages
are the retained deliverables.

## Native screenshots

Top office, middle corridors and the shared ramp shaft:

![Top office](../images/office-demo/01-top.png)
![Middle floor](../images/office-demo/02-middle.png)
![Connected ramps](../images/office-demo/03-ramps.png)

Basement mannequin running and resting, followed by a roach near its refuge:

![Mannequin running](../images/office-demo/04-mannequin-run.png)
![Mannequin idle](../images/office-demo/05-mannequin-idle.png)
![Cockroach](../images/office-demo/06-roach.png)

The same view before and during a Full lighting failure:

![Light on](../images/office-demo/07-light-on.png)
![Light failed](../images/office-demo/08-light-off.png)
