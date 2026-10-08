# Stage 6 measured costs and budgets — 2026-10-08 UTC

[Native gallery](README.md) · [Contracts](contracts.md) · [Handoff](handoff.md)

Apple M2 Pro / Metal, native1280×720 at logical640×360, matched hero cameras,
High filtering/Full lightmaps/reflections/bloom. Owned measurements are sequential;
no test, bake or build overlaps accepted compiler/caster/GPU samples. Source,
asset, binary, package and UTC identities are retained. Full means forced build
on a warm OS filesystem; no cold filesystem flush or device-independent guarantee.

## Offline work and useful reuse

[Raw full/development report](../../../debug-maps/art-style-hero/evidence/stage6-compile-costs-v1/report.json)
and [tracked command summary](compile-costs-summary.json) use the same frozen
compiler, twelve workers and real offline GPU probe capture.

| Path | Wall seconds | Peak process MiB | Result |
| --- | ---: | ---: | --- |
| before-full | 8.198 | 700.20 | forced full |
| after-full | 10.049 | 729.25 | forced full |
| after-full-repeat | 9.915 | 726.98 | forced full |
| development | 2.574 | 571.22 | forced full |
| development-reuse | 0.216 | 46.22 | verified reuse |

Both new full archives exactly match the accepted visual package. The new hero
costs about 9.98s versus8.20s for the old control on the current compiler; its
additional authored content is retained. The entry Stage5 frozen compiler's
single full control sample was10.35s/unchanged0.322s. These are different input/
tool contexts and do not establish a compiler speedup. The refinement is a
measured content cost; reuse is the workflow improvement.

The fast development path uses `--variants medium --workers12`, a separate
output and a Medium player. It costs2.574s versus a9.982s median full validation
build (both new full samples), about3.88× shorter. [Native Medium](development/medium/room.png)
is labelled development output; the final gallery remains High. Physical quality
settings/variants remain distinct cache inputs. Medium reduces supported solve/
reflection/atlas work; it never omits content or substitutes for High acceptance.

The [incremental matrix](incremental-summary.json) retains22 timed samples,
11 cases and independent forced references under
`debug-maps/art-style-hero/evidence/stage6-incremental-v1`. Setup asset copies and
independent correctness builds are excluded from timed samples. Every archive
matches the independent full output byte for byte, including all HDR atlas and
reflection mip bytes. Two repeats per case are a bounded desktop sample.

| Edit | Median wall seconds | Median peak MiB | Decision |
| --- | ---: | ---: | --- |
| unchanged | 0.330 | 47.01 | package hit |
| metadata | 0.749 | 91.75 | prepared hit |
| presentation | 0.759 | 91.64 | prepared hit |
| texture | 9.966 | 728.80 | full rebuild |
| material | 9.849 | 731.31 | full rebuild |
| model | 10.022 | 727.53 | full rebuild |
| geometry | 10.105 | 727.12 | full rebuild |
| light | 10.078 | 733.54 | full rebuild |
| entity | 10.151 | 729.68 | full rebuild |
| combined | 10.263 | 730.42 | full rebuild |
| prop | 10.197 | 729.57 | full rebuild |

Unchanged runs retain the exact package in0.330s. Metadata/exposure edits take
about0.75s, roughly13× less time than this full validation build; full integrity
verification and executable hashing still run. The executable identity phase is
about20.6ms in the first hero receipt, measured separately rather than free.
Catalogue/source/tool/capture/variant changes and individual added/removed/changed
dependencies receive useful cache reasons. Fog remains a physical miss. Missing
or corrupt provenance rejects both cache paths; forced builds bypass both.
[Native texture and light comparisons](incremental-native/verification.json)
are pixel-identical between their incremental and independent full archives.

Selective chair builds cost0.164/0.166s and emit the exact same GLB
([receipt](assets/optional-chair-build-times.json)). `--only` bounds authoring
work; the builder still regenerates an unchanged selection. No asset-generator
cache hit or automatic regeneration from model-source PNG edits is claimed.
A GLB embeds that real PNG, so its authoring build precedes normal map compilation.
Architectural PNG edits directly invalidate downstream solved lighting.

## Receiver cost isolated before optimization

The preserved release test executables alternate three before/after pairs against
the same immutable Stage5 original package and assets. Thirty-two204-triangle
chairs share one visibility mesh; one caster moves1.25cm every frame. Each case
has40 warmups and200 retained samples. The measured region is only
`DynamicScene::update_with_visibility`: visibility, probe queries and payload
updates, excluding GPU upload, command dispatch, logging and frame pacing.
[All samples and counters](moving-caster-paired-summary.json) remain available.

| Pair | Stationary before/after median ms | Moving before/after median ms |
| --- | ---: | ---: |
| 1 | 0.243 /0.239 | 8.337 /5.434 |
| 2 | 0.243 /0.240 | 8.311 /5.441 |
| 3 | 0.241 /0.243 | 8.303 /5.434 |

Median of moving medians is8.311→5.434ms, a34.6% reduction in this kernel.
Stationary medians stay around0.24ms. Reusing the first support-anchor probe
sample removes the identical second query; donor order and fallback stay exact.
All32 casters, one prepared mesh/model build and global receiver invalidation
remain. The residual5.43ms is substantial; there is no selective spatial
invalidation or whole-engine/FPS speedup claim. The Stage4 7.559ms measurement
included other update work and is not this denominator. Old/new native players
on the same frozen room/entities package are byte-identical, as are diagnostic
final and normal captures ([verification](runtime-parity/verification.json)).

## Real GPU work and trace context

Eight-second owned-PID Metal traces use the ordinary native capture scene/post
encoder and copy/readback. Analysis uses seconds1–7. Trace exports and untouched
proof images are under [GPU evidence](performance/gpu/); raw traces/XML remain
outside target in `debug-maps/art-style-hero/evidence/stage6-*-room-v1*`.
The first before trace has zero ordinary surface draws. The after trace also
presents scenes, giving two encoders per capture cycle. Its lower union/encoder
is therefore **not a paired improvement**. CPU telemetry includes readback/PNG/
pacing and cannot establish ordinary gameplay FPS, despite nonzero after draws.

| Trace context | Scene encoders in6s | Active GPU union /encoder ms | Metal time-median/peak MiB |
| --- | ---: | ---: | ---: |
| Before, capture only | 72 | 1.295 | 155.625 /162.953 |
| After, capture + surface | 144 | 1.082 | 168.297 /175.625 |

Command-buffer submission groups identify `presented-raw` capture and `presented`
surface terminals; exact buffer IDs join the GPU scene spans
([scope audit](performance/gpu/capture-scene-scope.json)). Before capture scene
median/p95 is1.051/1.210ms; after capture is2.008/2.201ms while concurrent surface
scene is1.030/1.167ms. Different acquisition/concurrent GPU contexts prevent
attributing these differences solely to assets. Scene spans exclude post/copy;
active union includes driver/capture work. Allocation also includes differing
surface context and driver resources; its12.6MiB difference is not assigned to
model textures. No presented-frame CPU/GPU total, hardware-general60FPS promise,
fragment overdraw or isolated skinning/upload/stall cost is established.

## Content inventory and warning budgets

Matched [before](budgets/before-room.json)/[after](budgets/after-room.json) audits
validate every package entry and native manifest/quality. They use actual final
capture submission receipts, even when window counters are zero. The earlier
zero-draw control inventory is [rejected for scene counters](budget-control-correction.json).
Vertices remain vertices; triangles count actual indexed triangle-list draws,
including depth-occluded geometry. Sky/reflection/emission duplicates/post/UI are
outside these base-scene counts.

| Inventory | Before | After |
| --- | ---: | ---: |
| submitted_triangles | 5,234 | 6,944 |
| draw_calls | 38 | 43 |
| visible_vertices | 23,048 | 28,118 |
| material_changes | 33 | 38 |
| package_bytes | 6,020,347 | 6,632,334 |
| world_texture_resident_bytes | 75,497,456 | 75,497,456 |
| cached_models | 16 | 21 |
| source_model_triangles | 5,300 | 6,188 |
| source_prop_decoded_bytes | 3,948,544 | 5,259,264 |
| lightmap_pages | 2 | 2 |
| lightmap_charts | 5,598 | 6,874 |
| lightmap_resident_bytes | 33,554,432 | 33,554,432 |

Two1024 atlas pages/fourRGBA16F layers retain33,554,432 resident bytes. Full
charts grow5598→6874; field495 slots/29,762 serialized bytes and two64px reflection
probes remain. Unique cached-model triangles differ from authored instance sums;
new prop placement adds1144 triangles plus120 per refined chair instance.
Five newly cached sheets add1.25MiB base decodedRGBA; GPU mip allocations are
separately reported by the native receipt. Transparency remains existing glass/
water/ghost plus alpha-tested leaves. Fragment overdraw is unmeasured.

Compiled static collision boxes stay64 and the navigation grid5934 cells; six
furniture extents are intentionally corrected locally rather than making visual
trim physical. Normal traversal endpoints/standing height are exact in compiled
before/after284-frame routes, and native eastward endpoints are9.0283/9.0223m,
within normal timestep variation ([movement evidence](movement/native-verification.json)).
Both runtime chair and ghost spawn, and bidirectional navigation passes.

[Warning budgets](budgets/hero-warning-budgets.json) are configurable hero/M2Pro
review thresholds:48 draws,8000 submitted triangles,8MiB archive/prop decoded
sheets,96MiB world textures,3 atlas pages and64MiB atlas residency. Current hero
passes all ([audit](budgets/after-room-warnings.json)). Thresholds allow bounded
headroom; they do not authorize increasing content indiscriminately.

Hard representation/safety bounds remain enforced: package512 entries/1GiB total,
2MiB manifest/256MiB entry, four switchable groups, runtime atlas representation,
model65535 vertices/6000 triangles/16 materials/1024px image and renderer allocation
limits. Source24M level vertices/131072 material slots and large instance/light
ceilings are defensive bounds, not reasonable scene targets. Builder1500-triangle
shipped-art ceiling remains;500 target/800 review are policy budgets. No limit
is raised, warning hidden, content dropped or validation weakened. Remaining
old guide contradictions and nonhero adoption belong to Stage7.

## Final provenance hardening

After measurement, final review added rejection of a wrong compiler-inputs role
or unsupported provenance revision before reuse. The [v2 binding](final-tool-binding.json)
records new executable identity and a required-current forced full build (9.908 s).
All 32 physical package entries remain byte-identical to the measured v1 bundle;
only manifest/build-input provenance changes. The final archive is 6,632,335 bytes,
one byte above the v1 inventory. Existing kernel, native and edit-matrix evidence
retains its recorded v1 tool identity; no extra performance campaign is substituted.
