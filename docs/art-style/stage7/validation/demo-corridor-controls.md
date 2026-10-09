# Demo corridor controls — preparation only

[Matched plan](demo-corridor-control-plan.json) adds three purposeful normal High
views in enclosed Demo rooms 7, 8 and 10. The existing 48-map normal-spawn,
49-geometry, six-theme quality/grid and map-journey plans remain unchanged. No map,
asset, camera implementation or tracked bench tool was edited. No Cargo, compiler,
bake, native or target process was launched by this preparation.

## Actual saved witnesses and cameras

[Chair witnesses](demo-chair-witnesses.json) use real `home:dining_chair` geometry,
unit scale, yaw 0, its actual bounds `[-.25,0,-.245]..[.25,.902,.245]` and the existing
eight inset anchors. Spawn key 9007 is an API key; the renderer assigns an actual
`DynamicId`, which the checker pins independently at the first capture. The chair
is a normal rigid runtime model, without a placeholder, scaling trick or special
renderer branch. Each selected floor centre is human and rat walkable in saved
navigation and lies on the authored -0.9 m corridor floor.

| Room | Saved nav cell | Chair floor position | Nearest C1 probe to real bounds centre | Before raw candidates, centre / eight anchors |
| --- | ---: | --- | ---: | --- |
| 7 | 166368 | `[60.900002,-.89999998,-5.5]` | 9.454699 m | 0 / all 0 |
| 8 | 152296 | `[62.300003,-.89999998,-13.299995]` | 8.121617 m | 0 / all 0 |
| 10 | 185531 | `[66.900002,-.89999998,5.100006]` | 2.796632 m | 6 / all positive |

The original room 8 rat half-body witness `[66.299995,-.82,-13.099998]` is only
0.1334 m outside the 4.060940 m support radius. A chair corner could reach support
there, so it is retained in the saved-nav audit but does not establish a missing
chair payload. The new room 8 point maximizes the nearest-probe deficit among
eligible saved human/rat cells with all real chair anchors unsupported. It is a
legitimate floor position with nearly 4.061 m of extra centre deficit.

The source walls leave room 7's clear X range `[60.3,61.7]`; room 8's clear Z range
`[-13.7,-12.3]`. The turn opens above the inner walls that stop at Z−12.3, while
the north wallpaper wall is Z `[-14,-13.7]`. Room 10's walls lie at Z `[4.1,4.4]` and
`[5.8,6.1]`, with the adjoining corridor open at its east end. These are ordinary
accessible rooms with practical fixtures 25–31 and two blind turns. The planned
cameras remain inside those clear corridors:

| Room | Eye XYZ | Yaw / pitch | Direction |
| --- | --- | --- | --- |
| 7 | `[60.900002,.80000002,-3.5]` | `0 / −31.985°` | North along room 7 toward the chair 2 m away |
| 8 | `[64.300003,.80000002,-13.299995]` | `−90 / −31.985°` | West along room 8 toward the chair 2 m away |
| 10 | `[68.500002,.80000002,5.100006]` | `−90 / −37.976°` | West along room 10 toward the chair 1.6 m away |

The manifests retain exact computed pitch values. Actual native images must show
the chair and corridor; field labels, model counts and uniforms cannot establish
pixel visibility. Preserve an obstructed view before recording a new manifest.

## Matched native before/after

Before uses the byte-pinned preserved C1 Demo archive and frozen
`stage7-runtime/normal-c1-v2/places` player. The package/compiler provenance is
superseded and is never required-current or relabelled as final acceptance. Its
actual catalogue and manifest dependencies must still match installed assets;
the read-only preparation audit confirmed compatibility. If that changes before
execution, record the incompatibility instead of silently substituting a package.
After uses the completed new final campaign's exact Demo output and C2 normal
player. Both use the same source, model, camera, 1280×720 drawable, normal High/Full
graphics, unit-scale rigid spawn and ready-frame 120/240 capture sequence.

For each `ROOM` 7, 8, 10, run the existing helper under the explicit native allocation:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tools/bench/capture_art_style_hero.py \
  --manifest docs/art-style/stage7/validation/demo-roomROOM-manifest.json \
  --package ACTUAL_PHASE_PACKAGE --binary ACTUAL_PHASE_NORMAL_BINARY \
  --asset-root . --quality high --entity-light-trace \
  --lighting-sequence docs/art-style/stage7/validation/demo-roomROOM-PHASE-sequence.json \
  --frames 360 --out NEW_PHASE_CAPTURE_DIRECTORY
```

`PHASE` is `before` or `after`. Their image paths are separate and immutable.
Retries require new preserved sequence paths and manifests. The capture helper
records actual binary/package/source/catalogue/camera/tool identities, applied
settings, image SHA and the real native result. Each executor command also needs
the primary's ordinary argv/UTC/exit/time/log receipt. No Focus injection is used.

The ignored
[native support checker](../../../../debug-maps/art-style-hero/evidence/stage7-validation/check_demo_native_support.py)
reads actual `[entity-spatial]` and `[entity-light]` blocks immediately before the
existing post-readback capture-attempt marker. It requires actual spawn success,
the uniquely matched model/centre and unchanged `DynamicId` over both captures,
normal settings, matching loaded Full archive identity and genuine capture hashes.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 debug-maps/art-style-hero/evidence/stage7-validation/check_demo_native_support.py \
  --room ROOM --phase PHASE --receipt PHASE_CAPTURE_DIRECTORY/manifest.json \
  --binary ACTUAL_PHASE_NORMAL_BINARY --package ACTUAL_PHASE_PACKAGE \
  --out NEW_PHASE_SUPPORT_REPORT.json
```

After additionally passes `--field-audit PASSED_FINAL_FIELD_AUDIT.json`, binding
the native package SHA to the passed fully resolved final field audit. Before
rooms 7/8 require an actual uploaded binding with `enabled:false`, all eight
`residual_irradiance_validity.w` values 0 and centre source `Unresolved`. Before
room 10 observes support and visibility without asserting absence from its zero
owner-label count. After all three require `enabled:true`, at least one actual
valid anchor, centre source `Prepared` and uploaded local bounds matching the real
chair. A nonnull spatial JSON block alone passes none of these support assertions.
The CPU centre trace uses ordinary compiled visibility; its combined legacy
coefficients remain distinct from final residual/direct shader reconstruction.

## Final fully resolved saved-field audit

The ignored
[raw audit helper](../../../../debug-maps/art-style-hero/evidence/stage7-validation/audit_demo_corridors.py)
decodes the actual manifest-hashed PLPF v3 and PLNV v1 records, with all class masks
followed by all region arrays. It checks real external dependency bytes/SHA and
the final source/tool/catalogue build-input identities. It uses the runtime's
strict f64 squared lattice-distance `<4` predicate at actual f32 saved cell centres,
without changing radius, validity, owner rules or visibility. It reports every
saved human/rat cell in rooms 7/8/9/10 for Medium and Full, plus the planned native
chair centre and all actual inset anchors.

Preparation independently reproduced the exact superseded C1 counts:
human 275/336 and 92/176 unsupported in rooms 7/8; rat 414/504 and 139/264. Room 9/10
raw unsupported counts are 0. The immutable
[read-only C1 report](../../../../debug-maps/art-style-hero/evidence/stage7-validation/demo-prephase-field-audit.json)
is historical defect evidence, not final success. Its command exited 0 because
it reproduced the required negative control and actual assets matched.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 debug-maps/art-style-hero/evidence/stage7-validation/audit_demo_corridors.py \
  --package assets/levels/places_demo.placesmap --role final \
  --campaign COMPLETED_FINAL_PACKAGE_CAMPAIGN \
  --compiler FROZEN_FINAL_COMPILER --out NEW_FINAL_DEMO_FIELD_AUDIT.json
```

Final requires all 48 supported campaign cases passed, unchanged Demo spacing/
radius/dimensions, retained valid labels for originally covered rooms plus the
recovered rooms, matched Medium/Full layout and labels, unchanged saved route
populations and no unsupported raw-radius centres along those saved human/rat
routes. Zero route cells cannot pass as recovery. Planned native floor centres
must remain walkable and their actual centre/anchors must have raw support.
Any remaining route deficit is reported as unresolved coverage through primary;
no assertion is silently altered to fit the first improved phase.

Positive raw support still does not establish a clear segment through actual
walls, opaque/movable visibility or final pixel response. The normal native
entity support reports and genuine image inspection supply those separate checks.
The existing source test using asset-less prop placeholders remains a focused
regression, not a substitute for this fully resolved final evidence.

For the six-theme quality/grid runs, retain the existing identity/position checker
and report actual spatial enabled state, anchor validity and centre source at
each endpoint alongside it. Off or absent package fields, independent Low lighting
and texture/lighting hybrids retain explicit fallback scope. A restored High
endpoint with an available field and supported chair must show real enabled/
valid-anchor/Prepared support; mere nonnull JSON is only object-binding evidence.

All these native/final-field commands remain unexecuted pending serialized
allocation and the new final compiler. Owned preparation files are released with
no active jobs or target/native custody.

[Preparation receipt](demo-preparation-checks.json) pins the JSON/one-off identities,
preserved package/player hashes, reproduced historical raw counts and successful
syntax/sequence checks. Seven in-memory checker contracts accept the intended
disabled/Unresolved and enabled/valid/Prepared cases and reject lost support,
unexpected before support or a wrong entity centre. They supply no native pixels
or execution claim. The existing tracked 45-test preparation suite is unchanged.
