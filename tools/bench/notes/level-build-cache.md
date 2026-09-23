# Level-build cost

This note records what a level load actually costs, what could be cached safely,
and why no whole-level build cache is implemented. It is written from
measurements taken during the original load-cost work; the device numbers come
from the `[level]` line the game prints on load under `LIMINAL_VERBOSE`.

A separate, runtime-owned lightmap cache does ship: `LightmapCache`
(`src/lighting/lightmap/cache.rs`) stores baked atlases under `cache/lightmaps/`
below the state root and reuses them while the content key is unchanged. It
covers the lightmap bake only; it is not the whole-level build cache proposed
below.

## What a level load does

`Renderer::rebuild_level_geometry` runs three stages, timed separately and
reported as `[level] ... built in X ms (lighting A + props B + surfaces C)`:

1. **Lighting bake** (`LevelLighting::bake`) — builds the room baselines, the
   fixture pools and the opening blends for the whole level.
2. **Prop instancing** (`resolve_prop_instances`) — for every placed prop:
   transform each model vertex into world space, sample the baked lighting at
   that world position, and append it to the per-(model, cell) batch.
3. **Static surfaces** (`build_level_geometry_mesh`) — floors, ceilings, walls
   and fixtures, emitted through the spatial bucketing pass.

Measured cost, release build:

| Level | props | build ms | lighting | props | surfaces |
|---|---:|---:|---:|---:|---:|
| `bench_chairs_400` | 400 | 5.7 | ~0.0 | 4.0 | 0.0 |
| `bench_chairs_1000` | 1000 | 12.5 | ~0.0 | 8.7 | 0.1 |
| `prop_stress` | 150 | 8.8 | ~0.0 | 3.9 | 0.1 |
| `asset_demo` | ~120 | 11.8 | ~0.0 | 5.6 | 0.4 |
| `level_1` | 0 | 1.5 | ~0.0 | 0.0 | 1.5 |

The `bench_chairs_*`, `asset_demo` and `level_1` rows are historical
measurements from levels that no longer ship; `prop_stress` remains a
regression fixture in `tests/fixtures/levels/`.

(Mac development machine, so absolute times are ~50–70× faster than the
PocketCHIP's ~1.8–2.3 ms per prop. The *proportions* are what matter here.)

Conclusions:

* **Prop instancing dominates.** It is the only stage that scales with placed
  props and it is the whole of the per-prop load cost.
* The lighting bake itself is negligible on the desktop and small on the device;
  the fixtures only matter through `lighting.sample`, which prop instancing
  calls once per model vertex per instance.
* Static surface emission scales with rooms and walls, not with props.

## What can be cached

| Candidate | Safe to cache? | Notes |
|---|---|---|
| Decoded GLB models | **already cached** | `PropAssets` keeps one `Rc` per model path for the session. |
| Warm-space (model-local) shading inputs | partially | A model's own vertex colours and UVs are placement-independent. |
| Transformed + lit prop vertices | **yes** | Depends only on the level JSON and the prop assets. |
| Static surface vertices | **yes** | Depends only on the level JSON. |
| Baked lighting grid | **yes** | Depends only on the level JSON. |
| GPU buffer uploads | **yes** | Follows from the CPU-side vertices. |

There is an important synergy with indexed submission: because instancing calls
`lighting.sample` once per *submitted* vertex and a model has ~30% fewer distinct
vertices than its flat triangle list, indexed submission reduces the per-prop
build cost by the same ~30% without any cache at all.

## Not implemented: whole-level build cache

### Proposed cache key and invalidation

The design that fits the existing level format, with no change to it:

```
key = hash(
    level file bytes (or the exact JSON string used),
    prop catalogue bytes (assets/catalog.json),
    for each model path the level references:
        model file bytes,
        texture file bytes,
)
```

Invalidation is total: any edit to the level, the catalogue, a referenced model
or a texture produces a different key. There is no partial invalidation to get
wrong.

Proposed storage: one file per level under a cache directory
(`assets/levels/.cache/<key>.lvl`), containing the already-built static vertex
buffer, the prop vertex buffer, the index buffer, the per-batch ranges and
bounds, and the `LightingSummary`. Loading it means `read` + `buffer_data`
instead of bake + transform + sample.

Risks:

* The cache must be *versioned with the build format*. A renderer change that
  alters vertex layout, batch structure or lighting must bump a format version
  or every existing cache entry becomes silent garbage.
* Community levels arrive as `.zip` packs and are imported; the cache key must
  then cover the packed bytes rather than a path, or an import could reuse a
  stale entry.
* A wrong cache is a *visual* bug on other people's machines, which is exactly
  the class of regression this task is trying to avoid.
* It does nothing for a level's first load, which is the cost that was measured.

The right moment for this is a dedicated change with its own format-version test
matrix, not a renderer-optimisation change. What this work does contribute is the
*instrumentation*: the `[level]` breakdown above makes the cost visible, and the
prop-instancing dependency on submitted vertex count means indexed submission
reduces it by ~30 % for free.

## Draw-order sensitivity of the spatial grid (measured)

Changing the spatial cell size leaves the *geometry* untouched — the 400-chair
stress level draws exactly the same 6,828 static vertices and the same 23,218
prop vertices with a 12 m grid as with a 96 m one (`props` and `render` unit
tests assert both, as multisets) — but it does change the **order** in which
those triangles are submitted, because ranges are emitted cell-major.

On a level whose geometry deliberately intersects itself (props sunk through the
floor, walls sharing faces) a different submission order resolves coincident
depths differently at a handful of edge pixels. Measured between a 12 m and a
96 m grid on the chair stress level: 676 of 522,240 pixels changed, in connected
components of at most 98 pixels, worst channel delta 67/255. Turning culling on
and off at a fixed grid changes nothing at all (0 pixels), and neither does
switching indexed submission or the vertex layout.

The shipping configuration uses the adaptive grid, and `visual_check.py` shows no
significant difference from the pre-optimisation build at that configuration.
The cell size is a tuning knob (`LIMINAL_CELL_METRES`), not a correctness
setting.
