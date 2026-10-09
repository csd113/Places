# Dense capacity package storage audit

October 9, 2026 UTC. The normal frozen C12 compiler campaign completed 47
supported maps and the two explicit offline controls, but the unchanged dense
fixture failed publication: 1,435,049,755 uncompressed bytes exceed the
1,073,741,824-byte aggregate guard by 361,307,931 bytes. The gate exited 1;
its final Rust/Python/native checks did not run.
[All successful archives and receipts are preserved](c1-v6-superseded-preservation.json).

The source remains SHA
`8ff5d19cdde7ef853e932de7e03b864fc178eb86e8ac99e138295e40c21845f9`:
5,312 props, 48 models, 2,760 solid props, 120 fixtures and 18 routes.
The writer rejected the aggregate before publishing a partial archive.
Historical three-variant success is reported, but no dense archive survives
among 1,274 inspected preserved manifests. Exact old Full atlas availability
cannot be inferred from that report.

## Actual original costs

The preserved ordinary compiler SHA
`c12e9c69ddd3bc5ecc04a24f8489f605158145d64423c29535fdac2937161d34`
built separate Off, Medium and Full packages serially in 315.231 seconds.
Each passed its actual required-current and full decoding commands.
These packages are diagnostic references; they do not satisfy the normal
three-variant gate. Their sources, catalogue and external dependencies are exact.

| Original variant | Prop record bytes | Individual archive uncompressed bytes | Atlas / irradiance |
| --- | ---: | ---: | --- |
| Off | 473,890,078 | 480,622,719 | None, as intended |
| Medium | 473,890,078 | 480,580,726 | Explicit page-overflow vertex-lighting fallback |
| Full | 473,890,078 | 480,494,474 | Explicit page-overflow vertex-lighting fallback |

The single deduplicated console warning from the failed combined build did not
establish Full atlas success. Both actual saved Medium and Full manifests record
page overflow and no atlas or irradiance. Their quality-specific vertex lighting
is retained. [Actual record/role costs](validation/capacity-original-cost-v1.json)
and the immutable single-variant archives remain outside target.

## Measured lossless storage choice

Whole-record content addressing already shares identical blobs; there is no
duplicate exact-blob bug. ZIP compression cannot reduce the uncompressed guard.
The complete 69-byte vertex contains position, colour, material UV, normal,
tangent, handedness and atlas coordinates/page. Distinct corners cannot merge
based only on position or an approximate normal.

An independent exact-byte oracle counted each actual variant. Every one has
1,016 batches, 6,665,147 vertex slots and 6,923,034 ordered indices. Sharing
identical complete vertices within their own batch retains 6,297,672 slots and
saves 25,355,775 bytes per record. Alone, this leaves the package at
1,358,982,430 bytes: still above the limit.

The remaining vertices contain 2,001,957 distinct 28-byte normal/tangent/
handedness frames across the batches. No frame value is rounded or normalized.
The maximum batch palette has 22,374 frames, so every measured batch can use
16-bit references. A palette stores each exact frame once; each vertex retains
its 36-byte position/colour/UV prefix, five atlas bytes and its frame reference.
The decoded runtime vertex and every ordered triangle corner remain identical.

The independent prediction includes the mode byte, palette count and every
frame/reference byte. Each prop record becomes 340,854,707 bytes, saving
133,035,371 bytes relative to the original. Across three variants, the projected
aggregate is 1,035,943,642 bytes, leaving 37,798,182 bytes below the unchanged
one-GiB limit. This is a measured storage prediction, not actual encoder,
package, native-memory or frame-time acceptance.

The bounded PLMP6 design keeps batch/material/caster/bounds metadata and ordered
u16 triangle indices. Each batch chooses literal 69-byte vertices, a u16 frame
palette or a u32 frame palette; a palette is selected only when strictly smaller.
Legacy PLMP3/4/5 records retain their documented defaults and caster semantics.
No shader, topology, UV, geometry/solver revision, content, quality variant or
archive/record/count cap is changed. Existing compiler identities invalidate
normally when the record version and executable change.

An explicit cumulative expanded-literal record budget must preserve the former
512-MiB prop allocation envelope, in addition to the encoded typed guard. A
smaller encoded representation alone cannot justify a larger decoded allocation.
The actual compacted dense record expands to 448,534,303 bytes in the old
literal layout. Actual CPU/GPU residency and transition peaks remain measurements.

## Acceptance still required

The implementation must pass strict source gates, literal/palette and genuine
legacy controls, exact signed-zero and all-attribute seam tests, malformed-mode/
count/reference/truncation/nonfinite rejection, and expanded-budget boundaries.
The real normal combined archive must fit one GiB, validate and be current.
The independent oracle must compare its ordered 69-byte triangle streams and
all unchanged metadata with the preserved originals. Full supported-map,
incremental/full, collision/navigation and native resource/quality evidence are
still pending. No fixture shrink, quality cut, cap increase, cache exception or
map-name condition is an accepted substitute.

## Actual combined archive — October 9, 02:47 UTC

[The ordinary combined command](execution/capacity-combined-plmp6-v1.json)
passed build, required currentness and decoding in 313.740 seconds. The actual
forced build took 303.866 seconds; those checks are separately timed in the
preserved report at
`debug-maps/art-style-hero/evidence/stage7-capacity-after-v1/report.json`.
The new compiler SHA is
`f3f27db928af9927219f3440a95acbbcfb2f53bdecb321af207613ddce91527f`.

The one normal Off/Medium/Full archive holds **1,035,943,642 uncompressed bytes**,
exactly matching the independent prediction and leaving **37,798,182 bytes**
below the unchanged aggregate limit. The physical archive is **334,585,036 bytes**.
Every actual prop record reports the same 6,665,147 input slots, 6,297,672 stored
slots, 367,475 exact duplicate slots removed, 1,016 batches and 6,923,034 indices.
The authored source hash remains unchanged. No content or quality variant is cut.

The exact-owned two-second sampler collected 151 observations, reaching
2,448,932,864 bytes of compiler RSS. This is a coarse observed process maximum,
not a runtime/driver memory result or an exact process-tree peak. The rejected
old combined build and three separate original builds are different contexts;
the elapsed times and RSS do not establish a performance improvement.
[Sampler completion](execution/capacity-plmp6-rss-v1.json) confirms it joined.

[Preflight v18](execution/rust-source-preflight-v18.json) passed strict debug
and release Clippy, all 23 codec controls and the two real-asset round trips.
The latter require exact ordered 69-byte corner streams, model/material/caster/
bounds metadata and canonical re-encoding; their atlas and shader controls remain.
The prior v17 failure of a redundant-slot assertion remains preserved.
Independent actual Off/Medium/Full oracle comparisons, the full inventory,
platform checks and native resource/quality acceptance are still pending here.

### Independent comparison completed

The first oracle attempt stopped before parsing because the original adapter
named the preserved copy instead of the actual output path. Its failed receipt
and original report remain immutable. The append-only `report-v2.json` names
the actual build/currentness/decode output; the original and preserved archives
independently match SHA
`1ba52aafb75b502f0d9c380cf4d7dca2c226f523fbe57b55e25735156051c0cc`.
No checker exception or changed command is used.

[Off](validation/capacity-off-plmp6-exact-compare-v2.json),
[Medium](validation/capacity-medium-plmp6-exact-compare-v2.json), and
[Full](validation/capacity-full-plmp6-exact-compare-v2.json) then passed serially
in 32.841, 32.799 and 33.047 seconds. Each actual 340,854,707-byte prop record
matches the independent whole-wire prediction and expands to 448,534,303 bytes
under the unchanged 512 MiB guard. All 1,016 raw batch headers and 6,923,034
ordered 69-byte corners match their original, including first-occurrence unique
slots, remapped indices and unreferenced slots. Source, catalogue, external
dependencies and every non-prop selected-variant record remain exact.

This establishes lossless storage and saves 399,106,113 uncompressed bytes across
the three variants. It preserves C12's Medium/Full page-overflow/NoField outcomes;
it cannot establish unpreserved historical Full atlas availability or native
memory, visual or frame-performance acceptance. The specialist joined all jobs
and released its allocation before platform checks began.
