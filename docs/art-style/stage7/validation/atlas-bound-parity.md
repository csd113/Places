# Stage 7 measured atlas bound

The ordinary v4 package campaign reached a real Demo Full atlas of
335,544,516 bytes and stopped because the typed bound was 268,500,992 bytes.
The primary stopped the owned gate at 2026-10-09 00:54:47 UTC; its
[receipt](../execution/normal-desktop-gate-v4-stop.json) preserves eight earlier
passed cases and the failed Demo command under
`target/verification/map-regression-20261009T002850Z-19070`. Those records remain
partial. A new compiler and completed ordinary campaign are required.

The approved limit is **335,609,856 bytes**, exactly 320 MiB +64 KiB, for atlas
records only. `tools/bench/scene_budget.py` uses one constant for declared-role
and typed lightmap bounds, matching the renderer-owned Rust package/compiler
contract. The existing general 256 MiB entry, 512 MiB mesh/props, 1 GiB aggregate,
512-entry count and all other typed limits remain unchanged. Every actual ZIP
member still needs matching declared bytes and SHA-256; duplicate, undeclared,
repeated and wrong-role records retain their existing failures.

The actual shape explains the encoded size: ten pages × two planes × two
illumination groups = 40 RGBA16F layers, each 1024 ×1024 ×8 bytes. The texels
occupy 335,544,320 bytes; the canonical KTX2 header/index/descriptor contributes
196 bytes. The focused test constructs only that header and metadata-size
witness, never a 320 MiB payload or image. It checks the literal shape arithmetic,
the measured record, the exact ceiling and the one-byte overflow. Small actual
ZIP witnesses independently reject modified header bytes and appended bytes
through the unchanged hash/size checks.

Python continues to perform bounds preflight and archive integrity checking.
The ordinary Rust decoder owns legacy RGBA8 and current RGBA16F acceptance,
single-image/array semantics, header/descriptor integrity, faces/levels,
offset/range checks and complete texel decoding. No additional Python format
filter or alternate decoder was introduced. These focused tests do not claim
to validate a complete 320 MiB atlas, malformed decoder payloads or native
rendering; the ordinary compiler gates and Rust decoder tests supply those
checks.

Decoded f32 lighting and moments for the measured shape occupy **640 MiB**.
The decoded arrays, encoded half-float container and another codec texel copy
can together approach **1.25 GiB** before world/prop meshes, actors, staging or
driver allocations. That estimate describes components, not measured process
RSS or GPU allocation. Forty layers fit below the existing 256-layer reference
adapter limit, but the final actual adapter capability and allocations still
require genuine native receipts. Page, edge, switch-group, ordinary-entry and
aggregate safety bounds were not widened.

The [focused result](atlas-bound-parity-checks.json) records 20 passing Python
tests. Final acceptance still requires a completed stable 48-map ordinary
campaign, package aggregate/currentness/decoder validation, exact installed
incremental/full comparisons, retained original availability, resolved Demo
field/navigation audit, frozen normal C2/diagnostic players and the already
prepared serial native campaign with actual resource and process RSS receipts.
The old native plan is a preserved `ready:false` specimen and must be freshly
generated using the final helper and binary identities.

Validation started no Cargo, compiler, bake, native or target job during this
allocation. The allocated helper, test and documentation files are frozen and
released to the primary.
