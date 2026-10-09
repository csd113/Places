# Demo HDR atlas record audit

The actual normal C1-v5 Demo build failed after 332.427 s because a 335,544,516-byte atlas exceeded the 268,500,992-byte typed record guard. No new Demo package was published; final package/native acceptance remains pending.

The exact failure size is 320 MiB of HDR data plus 196 bytes of KTX2 header: ten 1024px pages, two RGBA16F planes, base plus one switch contribution. This is 40 GPU array layers and 320 MiB resident HDR data. CPU float pages require 640 MiB. Original entry Full had eight 1024px pages with the same one-switch layout, 32 layers and 256 MiB + 196 bytes; Medium had 5 pages. The earlier preserved C1 had Medium seven / Full eight pages. Every preserved atlas payload SHA is independently streamed and verified.

The current Demo source SHA `65833e98579a696167d616ce0cc94ebab85f744fdd72837ed247c483db1f9dec`, catalog SHA `27789ac9a59ec79decb5c95e63f6a4b0de3bbd5101f4ae3d2d45f177e82bee1f` and all 148 dependency sizes/hashes exactly equal the preserved C1 inputs. The failed current physical mesh was not serialized, so no current mesh-byte equality is asserted.

The minimal recommendation is an explicit typed atlas memory-budget increase from 256 MiB + 64 KiB to 320 MiB + 64 KiB (335,609,856 bytes), a 25% payload increase. The atlas guard changes; ordinary 256 MiB entries, aggregate 1 GiB archive guard and strict shape/metadata/checked arithmetic remain. Existing KTX payload cap 512 MiB already supports this record. The measured 40 layers remain within device 256 layers; runtime ten pages / four-switch cap permits at most 100 layers.

Actual failed-build aggregate bytes are unavailable: all Off/Medium blobs and Full products lived in process memory, and failure occurs before manifest construction and atomic write_archive. There is no persistent prepared-stage cache. From the exact prior C1 aggregate 687,760,188 bytes, atlas-only Full8to10 and worst Medium7to8 project 788,423,484 bytes, leaving 285,318,340 bytes below 1 GiB. Other repaired products can change; this projection is not a final measured total. The unchanged archive writer must evaluate actual complete entries before publication.

Decode components can retain 320 MiB encoded input, 320 MiB KTX level copy and 640 MiB decoded float pages, approximately 1.25 GiB before geometry, driver resources or a previous active world. Encoding has analogous float/layer/combined-record components. Actual peak RSS and native transitions require measurement. The existing 128 MiB memory cache refuses these oversized entries.

A packaging-only code edit changes executable SHA, and existing fingerprint_with_build_inputs binds that SHA into both full package and reusable prepared-stage fingerprints. C1-v5 prepared packages cannot satisfy the next compiler identity. Preserve them as superseded evidence and rebuild with the actual normal compiler; no stale tool exception is justified.

Separate stale diagnostic contracts: transport/diagnostics.rs rejects chart.page>=8 and cumulative receivers beyond8×1024². Both must use the shared supported maximum10; retain checked bounds and negative controls. The existing real_full_demo_atlas_loads_with_its_ktx_container_overhead test pins eight pages and needs the actual final ten-page witness. Scene-budget validation has two atlas 256 MiB + 64 KiB call sites and an owning boundary test; validation owner must update them consistently.

No chunked storage or schema revision is recommended. It would change manifest topology and reader/capture/load compatibility; the measured record already fits the codec’s strict shape envelope. New typed-bound positive/one-byte-over controls, original ordinary 256 MiB rejection, successful actual aggregate/currentness and native memory evidence remain required.

[Exact hashes, source records and failed receipt](integration-demo-atlas-record-audit.json). Evidence collection was read-only. The subsequent explicitly allocated source fixes are listed below; this specialist launched no Cargo/compiler/bake/native/target/Git actions.

The allocated fixes are implemented and source ownership is released. The owning typed bound is 335,609,856 bytes; the atlas memory guard is explicitly raised, while ordinary entries remain 268,435,456 bytes and aggregate uncompressed archive entries remain 1,073,741,824 bytes. Focused tests cover old and new records, ordinary-role rejection, typed one-byte-over rejection, actual supported diagnostic page boundaries and the cumulative sample boundary. No pixel buffers or receivers are allocated by the new diagnostic controls.

The artifact-backed Demo test remains strict: 335,544,516 bytes, ten pages, one switch contribution and 40 layers, with validation free of warnings and ordinary256MiB read rejection. Its actual new package, total aggregate and native proof are pending; no superseded archive is accepted as a replacement.
