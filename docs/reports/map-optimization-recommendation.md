# Bringing map storage and browser delivery costs down

11 October 2026 · audit of `b1d2a1d6903e027f53435baa3577881b1866f6ac`

## Recommendation

**Keep the current look and HDR. Make map data independently fetchable by quality,
reuse assets by their existing hashes, and share the repeated parts of prepared
geometry. Retain `.placesmap` for offline import and complete map-plus-assets
exports.** This is the
best first project for browser delivery: it avoids transferring work the player
does not currently need, without changing lighting or rewriting the renderer.

The measured eight-map collection is **560.90 MB of packages**, plus **68.29 MB
of unique declared assets**. Fetching just each map's existing Full-quality records
would require approximately **319.09 MB of map data**: **43.1% less initial map
transfer**, before application overhead. All qualities remain available on demand.
That defers bytes; a complete offline collection still needs them.

For a major reduction in *complete installed map storage*, lighting must also
change its encoding. Exact prop sharing alone saves **39.35 MB / 7.0%** across
the current collection. A separately approved compact HDR encoding trial is the
next substantial opportunity. Earlier 12-bit experiments suggest promise, but
are lossy and are not fresh proof for this lineup. Increasing ZIP compression is
low priority: it mostly saves a few percent while making packing much slower.

This investigation changes only documentation and isolated audit tools. Production
packages, maps, artwork, quality settings, renderer and public format are unchanged.
The recommendation requires a later implementation decision; the audit is complete
without that decision.

## What is large today

**Measured current files**, with all entry hashes and installed dependency hashes
checked. MB means 1,000,000 bytes; MiB means 1,048,576 bytes. Raw records are the
sum of inflated ZIP members, not the player’s memory consumption.

| Curated map | Archive MB / MiB | Raw records MB | External assets MB | Rooms / placed props |
| --- | ---: | ---: | ---: | ---: |
| Home | 6.24 / 5.95 | 71.96 | 16.11 | 4 / 18 |
| Pool | 17.85 / 17.02 | 74.84 | 1.78 | 3 / 20 |
| Frutiger Aero | 26.40 / 25.18 | 156.46 | 1.83 | 4 / 620 |
| Beach | 32.36 / 30.86 | 276.88 | 1.32 | 1 / 441 |
| Winter | 80.04 / 76.33 | 310.35 | 15.47 | 44 / 310 |
| Demo | 103.86 / 99.05 | 746.31 | 51.60 | 14 / 1,617 |
| Model Zoo | 136.15 / 129.84 | 376.96 | 19.22 | 38 / 275 |
| Lantern Hollow | 158.00 / 150.68 | 557.66 | 14.29 | 54 / 1,076 |

The eight packages contain 2,571.17 MB of inflated records. Required assets total
121.62 MB when counted separately for every map, but only **68.29 MB once** across
the collection. Shared installation avoids **53.32 MB** of repeat asset copies.
Whole prepared-record hashes offer **zero additional cross-map payload sharing**
in this lineup. All eight maps have three prepared qualities; identical records
within each package are already stored once.

The existing runnable export includes 90.46 MB of non-map assets, including the
catalog, startup/UI and other shipped assets. Thus its asset tree plus all maps is
**651.36 MB**, excluding executables/licenses. The narrower **629.19 MB** map-plus-
declared-assets figure is not a complete app download. These are logical unique
file lengths, not uniquely reclaimable APFS blocks, Git history or build caches.

The following accounts for every compressed archive byte in representative maps.
“Other” includes archive structure, manifest, semantics, irradiance fields,
reflection probes, mesh, collision and navigation.

| Map | HDR atlases MB | Prepared props MB | Chart metadata MB | Prepared lighting MB | Other MB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Home | 5.39 | 0.59 | 0.13 | 0.06 | 0.07 |
| Demo | 67.79 | 18.78 | 4.94 | 9.58 | 2.77 |
| Model Zoo | 120.51 | 8.66 | 1.52 | 2.80 | 2.65 |
| Hollow | 99.20 | 41.07 | 13.22 | 2.33 | 2.18 |
| Winter | 54.84 | 16.59 | 6.25 | 1.14 | 1.23 |

Lightmaps occupy 65–89% of most curated archives. Zoo demonstrates why geometry
sharing alone is insufficient: its 275 placements use 262 distinct models, while
lighting occupies 88.5% of its archive. ZIP headers/indexes occupy only a few KB.
Collision/navigation together occupy 0.002–0.068 MB per curated map; deleting
essential gameplay data would provide negligible savings.

### Why apparently modest scenes grow

**Raw GLBs and PNGs are already external path/hash dependencies.** The package
stores decoded, transformed and prepared prop geometry for each placement and
quality, not another original GLB file. GLBs contain their own embedded model
images; surface, fixture and sky PNGs are separately verified dependencies.
Externalizing raw models again is not a new solution. See
[prop serialization](../../src/engine/package/props.rs) and
[dependency collection](../../src/map_compiler/mod.rs).

Each lightmap texel has two RGBA16F planes: RGB irradiance and a signed directional
moment, with reserved alpha. A 1,024-square page pair costs **16 MiB per lighting
group**. An independently switchable light adds another pair for every page.
Demo retains eight Medium and ten Full pages, with two groups: **576 MiB of
atlas pixels** across both qualities. Larger lit surface area, more receiver
charts and more independent groups grow lighting; page allocation adds steps.
Increasing chart resolution increases area approximately with the square of the
resolution change. Room count or polygon count alone cannot predict map size.

Repeated model placements grow expanded vertices and instance-specific lighting.
Receiver seams and Low-quality baked vertex colors prevent simply reusing every
vertex as-is. PLMP6 already shares exact vertices and normal/tangent palettes
inside a batch. Further sharing should separate reusable mesh data from transforms,
lightmap coordinates and placement lighting. See
[preparation](../../src/engine/render/common/props.rs).

Atlas occupancy is not an easy archive saving: Full Home is 75.4% unallocated;
Demo 19.8%; Hollow 11.9%; Zoo 7.8%. Empty pixels compress cheaply. Hollow also
uses 33.0% of atlas area for sampling gutters, which cannot simply be removed.
Repacking charts or sparse tiles must preserve all occupied/gutter texels, chart
coordinates and switch groups. No occupancy saving was prototyped here.

## What the isolated prototypes prove

**Fresh lossless measurement using the project’s release zlib-rs Deflate codec.**
Baseline level-6 repacks matched original archives byte for byte. The shared-prop
prototype splits vertex streams, shares exact chunks across qualities, and retains
headers, palettes and ordered indices. Two independent native codec outputs were
identical. Reading each resulting research ZIP reconstructed **every original
entry byte**, including signed zeros, lighting, probes, collision and navigation.

| Map | Current bytes | Shared-prop research ZIP bytes | Saving | Deflate 9 saving |
| --- | ---: | ---: | ---: | ---: |
| Home | 6,244,243 | 6,027,804 | 3.47% | 7.22% |
| Pool | 17,845,535 | 17,652,191 | 1.08% | 4.16% |
| Frutiger Aero | 26,400,970 | 25,510,019 | 3.37% | 4.52% |
| Beach | 32,364,170 | 31,371,472 | 3.07% | 5.26% |
| Winter | 80,037,202 | 72,850,520 | 8.98% | 2.75% |
| Demo | 103,859,345 | 96,367,737 | 7.21% | 4.52% |
| Model Zoo | 136,149,851 | 132,128,186 | 2.95% | 2.87% |
| Hollow | 157,997,935 | 139,641,993 | 11.62% | 2.42% |
| **Collection** | **560,899,251** | **521,549,922** | **7.02%** | |

These are intentionally **unplayable research ZIPs**, not a new runtime format.
Their original manifests make current readers reject the undeclared layout.
Byte identity proves unchanged interpreted data; no screenshot claim is needed.
No combined sharing/lighting/streaming saving is claimed.

For Demo, native packing took 2.70 seconds at Deflate 6 versus 13.49 seconds at 9;
bounded all-entry decompression took 0.563 versus 0.744 seconds. Hollow packing
was 3.91 versus 14.27 seconds; decode 0.744 versus 0.769 seconds. These are offline
warm-OS-cache measurements, excluding semantic decode/GPU upload; read values are
medians of three runs. Higher compression does not reduce resident GPU memory.

The scratch factoring encoder plus native repack took about 5.3 seconds for Demo;
Python on-disk reconstruction about 1.6 seconds, peaking at **1.78 GB RSS**.
This full-copy proof needs a bounded streaming implementation before claiming
better load time or memory. Native baseline repack RSS was 672 MB for Demo and
426 MB for Hollow. It is not a browser decoder benchmark.

Earlier evidence remains a hypothesis: the 9 October artificial 5,312-prop stress
fixture shrank 334.59 → 154.34 MB through sharing (53.9%); it is not a curated map.
Its preserved package now has old dependency paths, so it is not a current native
runtime candidate. Earlier atlas byte shuffling saved Demo bytes but enlarged
Hollow/Winter. Earlier B12 lighting quantization saved roughly 18–30% of complete
packages with little change in sampled native views; B8 showed snow bands. Those
lossy results were not repeated on the current lineup, and their runtime path
expanded back to RGBA16F. Retaining HDR range differs from retaining exact stored
precision. None justifies dropping HDR, groups, probes or lighting quality.

## Download, load time and memory are different costs

The selected-quality column sums actual compressed Full-quality members, common
records and the entire original ZIP index overhead. It is a **transport estimate**,
requiring a new asynchronous fetch/reader path. It does not remove other qualities.
Dependencies below assume no required hashes cached; app/bootstrap bytes are extra.

| Map | Cold complete map + assets MB | Proposed cold Full + assets MB | Full map data with assets already cached MB |
| --- | ---: | ---: | ---: |
| Home | 22.36 | 19.70 | 3.59 |
| Demo | 155.46 | 113.35 | 61.74 |
| Model Zoo | 155.37 | 100.30 | 81.08 |
| Hollow | 172.29 | 96.33 | 82.04 |
| Winter | 95.51 | 58.97 | 43.50 |

After downloading Demo’s assets, Hollow needs only **2.57 MB of new dependencies**,
Winter another **8.84 MB**, and Home none, in that order. An unchanged revisit may
need zero resource body bytes if all cached content survives; decode/upload may
still be needed if decoded or GPU resources are no longer resident. Browser
storage can be evicted. Cache reuse is not guaranteed,
and moving bytes into a separate bundle does not eliminate the cold download.

**Fresh native baseline:** Apple M2 Pro, 16 GiB RAM, Metal, 1,280×720 drawable,
High/Full. Three alternating cold/warm application-state repetitions per map,
each in a new process; OS/GPU caches were untouched. All 30 runs presented the
requested playable scene. RSS measures the whole process on unified memory,
not isolated CPU map data or independently measured GPU peak.

| Map | First playable scene, cold / warm seconds | Peak process RSS, cold / warm MB | Full atlas GPU MiB | World/prop vertex + index allocation MB |
| --- | ---: | ---: | ---: | ---: |
| Home | 1.55 / 1.50 | 526 / 525 | 32 | 1.48 |
| Demo | 4.70 / 4.49 | 2,068 / 2,072 | 320 | 29.18 |
| Model Zoo | 3.25 / 3.27 | 1,540 / 1,543 | 176 | 14.45 |
| Hollow | 3.61 / 3.63 | 1,677 / 1,666 | 176 | 57.12 |
| Winter | 2.77 / 2.68 | 1,117 / 1,076 | 112 | 26.51 |

The player decodes **only the selected quality**, so fetching fewer qualities
saves transfer rather than eliminating current decode work. Loading inflates KTX2,
copies its pixel payload, expands half-floats into f32 CPU texels, then converts
them back to half-floats for upload. CPU atlas texels alone take twice the GPU
atlas bytes. Repeated batches also clone decoded prop image pixels; GPU texture
uploads already share identities. Removing these copies could improve memory
without changing artwork. See [loading](../../src/game/app/loading.rs),
[package decode](../../src/engine/package/world.rs), [lightmap decode](../../src/engine/package/lightmaps.rs)
and [upload](../../src/engine/render/wgpu/lightmap.rs).

Known allocations are components, not a total GPU peak: ordinary textures,
characters, reflections, render targets, mipmaps and old/pending worlds add more.
Per-batch prop texture statistics can count a shared resource repeatedly. Neither
lossless disk sharing nor B12-to-RGBA16F decoding proves GPU-memory savings.
Current Cargo features exclude browser backends; SDL3/native initialization and
the five-bind-group pipeline also need a browser port. No Internet transfer,
browser first-play, WASM decode or portable GPU codec trial was performed.

## What other engines actually demonstrate

**Source 1:** Valve’s [static-prop lump](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/public/gamebspfile.h#L133-L209)
uses a model dictionary plus placement origins, angles and model indices;
[VBSP writes those separate lists](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/utils/vbsp/staticprop.cpp#L498-L520).
Repeated placements do not store another complete transformed mesh in that lump.
[VRAD nevertheless stores unique per-instance vertex lighting](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/utils/vrad/vradstaticprops.cpp#L1395-L1533)
in `.vhv` payloads in the BSP pak. This supports sharing geometry while retaining
placement lighting. The [BSP pak lookup contract](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/public/bspfile.h#L313-L337)
also uses installed game/base assets. A fair comparison includes those required
models/materials/textures/sounds, counted once. The reported “about 50 MB Half-Life
2 map” is not a verified matched benchmark. These are Source 1 SDK findings;
Source 2 storage and map totals were not audited.

**Modern engine:** Epic’s [Instanced Static Mesh](https://dev.epicgames.com/documentation/en-us/unreal-engine/instanced-static-mesh-component-in-unreal-engine)
shares mesh assets among transforms. [Virtual texture lightmaps](https://dev.epicgames.com/documentation/en-us/unreal-engine/virtual-texturing-settings-and-properties-in-unreal-engine)
and [streaming virtual textures](https://dev.epicgames.com/documentation/en-us/unreal-engine/streaming-virtual-texturing-in-unreal-engine)
make tiles resident as needed, with borders, extra sampling and possible reactive
popping. This is a useful later memory strategy, not proof of fewer installed bytes.

**Browser-oriented assets:** Khronos [EXT_mesh_gpu_instancing](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Vendor/EXT_mesh_gpu_instancing/README.md)
provides transforms/custom instance attributes and recommends spatially sensible
batches. [EXT_meshopt_compression](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Vendor/EXT_meshopt_compression/README.md)
uses independently decodable buffer views and a WASM decoder. Its lossless vertex
codec is distinct from optional quantization/filters; Places would need exact
round-trip proof and measured decode. The newer
[KHR form](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_meshopt_compression/README.md)
is currently release candidate, so EXT is the established comparison.

[KTX2](https://github.khronos.org/KTX-Specification/ktxspec.v2.html#_supercompressionscheme)
supports supercompression. [Basis Universal now supports HDR](https://github.com/BinomialLLC/basis_universal#supported-hdr-gpu-texture-formats),
but these HDR codecs are lossy. The [glTF Basis extension](https://github.com/KhronosGroup/glTF/blob/main/extensions/2.0/Khronos/KHR_texture_basisu/README.md)
alone does not establish arbitrary HDR lightmap support. [WebGPU texture formats](https://www.w3.org/TR/webgpu/#texture-formats)
require optional BC support for BC6H; ASTC browser formats are UNORM/sRGB, not
ASTC HDR. Signed directional coefficients need explicit handling. A portable
trial needs feature detection and a half-float fallback; fallback expansion
does not lower atlas GPU memory. Codec names cannot substitute for visual proof.

## Ranked implementation batches and decision boundaries

1. **Selected-quality delivery and verified shared assets.** Add a small manifest
   index and independently compressed, immutable hash URLs, or an asynchronous
   ZIP range reader. Preserve all qualities and complete offline map-plus-assets
   export/import. Current imported maps require installed assets; the compiler
   does not create standalone custom-asset packages. Add explicit bundling/import
   support if authors need one-file sharing, including custom GLBs as well as
   textures; extract verified reusable content into an explicit user cache. Count
   relocated bytes in cold totals. Pin dependency versions/hashes and compatible
   catalog identity; report missing/mismatched assets clearly. This is the first
   browser architecture decision, with the measured 43.1% Full-only map-transfer
   opportunity above. It does not by itself solve complete installed storage.
2. **Lossless storage and memory foundations.** Add versioned shared prop streams
   and compact chart metadata, preserving every chart and diagnostic/export API.
   The measured prop saving is 7.0%; chart compaction is unmeasured. Separate
   solved-data cache identity from encoding identity to avoid needless rebakes.
   Share decoded images per model and reduce atlas conversion/copy lifetimes.
   Require deterministic clean/incremental output, exact reconstruction, bounded
   allocations, native quality switching and current-world memory measurements.
3. **Compact HDR lighting pilot.** Compare exact channel/sparse/block encodings
   with the smallest original representation, then separately trial B12 or a
   signed/HDR GPU codec. Preserve range, direction, interpolation, gutters,
   independent groups and probe/entity lighting. Any quantization or changed
   interpretation requires an explicit quality decision and matched native views
   across exposure, snow, colored lights, switches and multiple GPUs. This is the
   strongest route to substantial complete-package savings, still unproven for
   the current lineup. Do not add historical savings to the lossless table.
4. **Shared prepared meshes and spatial streaming where justified.** Store model
   geometry once, transforms per instance, compact placement lighting separately;
   retain normal transforms, receiver seams, collision and culling. This resembles
   Source’s useful separation and can reduce CPU/GPU geometry growth. It is a
   larger compiler/renderer migration, with no measured Places size result here.
   Add tiled lighting/spatial streaming only when a runtime pilot demonstrates
   worthwhile residency/first-play gains. Keep geometry codecs selective and
   reject encodings whose decode/size tradeoff is worse. Deflate 9 ranks last.

Keep existing safe paths, hashes, typed limits and capabilities. New readers need
checked offsets/counts, bounded reconstruction and aggregate working-memory caps,
not just compressed-size limits. New representations must fail clearly in older
players; compatible new readers should preserve existing imports or provide an
explicit migration tool. Current format accepts one package major version, so
backward support would be new work, not an existing guarantee.

Deployment also matters: [HTTP Range](https://datatracker.ietf.org/doc/html/rfc9110#section-14.2)
can be ignored by a server, and transport content coding changes the served byte
representation. Use validated fallback behavior and suitable CORS. Separate full
chunk responses simplify caching: [Service Worker Cache.put](https://www.w3.org/TR/service-workers/#cache-put)
rejects raw 206 partial responses. [Immutable versioned URLs](https://datatracker.ietf.org/doc/html/rfc8246#section-1)
help reuse; [browser storage](https://storage.spec.whatwg.org/#storage-pressure)
still needs quota/eviction handling and an offline completeness check. Fetch only
from approved bundle locations; map content must not authorize arbitrary network
downloads or executable content. Verify hashes after bounded download/decode.

## Evidence and reproduction

Current inputs passed native validation, all entry/dependency SHA checks, exact
baseline repacks, deterministic factoring and complete on-disk reconstruction.
The focused proof tests cover changed bytes and invalid chunk references. The
strict Rust/Python gates and publication receipt belong to the final handoff.
No whole environment rebake was requested for these measurements. Required Rust
regressions independently bake their normal fixture cases.

Useful local evidence is retained under ignored
`tools/bench/results/map-optimization-20261011/`: `delivery.json`,
`current-provenance.json`, `native-summary.json`, `native-load/`,
`lossless-native/campaign.json`, per-entry
proofs and gate logs. Original package/binary SHA identities are recorded there.
The previous storage/HDR evidence remains intact under its dated result roots;
its untracked reports/images are preserved and are not published as a second
recommendation document. Temporary STORE/repeated ZIPs from this campaign are
removed only after proof; compressed research ZIPs and receipts remain local.

From the repository root, with current compiled maps and release tools:

```sh
scratch=tools/bench/results/map-delivery-reproduction
mkdir -p "$scratch"
python3 tools/bench/map_storage.py --self-test
python3 tools/bench/map_delivery.py --out "$scratch/delivery.json"
RUSTC_WRAPPER= cargo build --release --bin package-repack
python3 tools/bench/map_delivery_campaign.py \
  assets/levels/home_showcase.placesmap assets/levels/places_demo.placesmap \
  assets/levels/model_zoo.placesmap assets/levels/lantern_hollow.placesmap \
  assets/levels/winter.placesmap --out "$scratch/lossless"
python3 tools/bench/loading.py --binary target/release/places \
  --out "$scratch/native" --levels home_showcase places_demo model_zoo \
  lantern_hollow winter --repeat 3 --timeout 90
python3 -m unittest discover -s tests -p 'test_map_delivery.py'
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Use new output directories. Native display access and macOS process counters may
require the host’s normal execution permission. Do not run timing campaigns
concurrently with builds. Browser throughput, total GPU peak, new runtime loaders,
current-lineup lossy quality and Source 2 comparisons remain **not tested**, not
reasons to stall the completed audit or the next queued task.
