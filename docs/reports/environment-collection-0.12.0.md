# Places / The Quiet Collection — 0.12.0 staging

Status: the final nine-map Full package is exported and verified current, with
all twelve editable source snapshots recovered byte-exactly. All thirteen essential final
native cases passed (1,131 checks), followed by a normal Hub startup after Cargo
cleanup. The tested launch command is in
`local/environment-collection-0.12.0/normal-hub-post-clean/command.txt`. The collection
and supporting tooling are committed in `d2ec9971`. The subsequent
[distribution handoff](environment-collection-0.12.0-retirement.md) retires the
standalone demos and records current validation, launch commands and deferred work.
The package is 449,857,977 bytes (449.857977 MB / 429.017999 MiB). The accepted
compile/bake/export took 5,770.880 seconds; supervisor-observed wall was
5,771.838 seconds. Detailed per-map size/phase timing and cache decisions are in
`local/environment-collection-0.12.0/export-metrics-final.json`.

One editable project, `projects/environments`, contains a central hub and eight
environments. The hub has eight physical doors with destination materials,
recognizable props and readable nameplates. Each destination has a physical
return door. A fresh aimed E press changes maps; arrivals face away from the
return leaf and do not trigger automatic bounce loops. Each visit starts fresh
objectives, scripts, doors and physics; held objects stay in their environment.

| Previous distributed map | Collection destination | Change |
| --- | --- | --- |
| `home_showcase` | Home / Quiet Rooms | Complete seven-room domestic redesign; R2 cushion quest |
| `pool_showcase` | Pool / Still Water | Second pool hall and basin; separate furnished janitor closet |
| `office_demo` | Office / After Hours | Basement ramp moved to west shaft, reached through middle level |
| `retro_arcade_demo` | Cosmic Lanes & Arcade | Dormant physical tabletop items, lane and object resets |
| `lantern_hollow` | Lantern Hollow | Reviewed import, physical props, trail light interaction |
| `winter` | Winter / Snow & Shelter | Reviewed import, existing weather controls and lodge route |
| `beach_demo` | Beach / Beyond the Shore | Reviewed import, physical crates and crab interaction |
| `frutiger_aero_demo` | Frutiger Aero | Reviewed import, kiosk ambience interaction |

All eight originals were imported through the actual editor `Document` operation,
which is also exposed as Project > Import JSON. Baselines preserve exact source
bytes, stable identities and placement metadata. A narrow imported Pool fixture
exercised preview/apply/undo, saved/reimported placement ignores and both direct
and batch autofix refusal for a protected duplicate. Production maps received no
blanket ignores. The checker now recognizes Arcade's legitimate animated balloon
placements instead of reporting 34 false duplicate-prop warnings. Pool coping
joints required small deliberate geometry edits: wall-only autofix cannot safely
infer their intended shape. See [import evidence](environment-0.12.0-import-audit.md).

The mannequin receives the original held cushion atomically, attaches its actual
render resource to R2's animated `hand.R` node, thanks the player and walks to an
opaque service corridor. Wrong objects remain held; repeat dialogue cannot hand
over twice. Reset restores the original item identity, dormant pose, NPC,
interaction bounds and quest state. Group operations refuse NPC-owned items.
Retirement requires one actual opaque, uncut wall to cover the eight projected
corners of the entire current posed character and carried-prop bounds, with a
safety margin. Both meshes are hidden before retirement is confirmed. This
conservative proof can keep an actor waiting where several occluders jointly hide
it; it never relies on a torso point, timer or looking away. The Home corridor is
authored to provide the required complete occluder.

Narrow native preflights passed with direct executables and focus checks: Home
wrong/repeat (62 checks), success/hidden retirement/reset/fresh request (81), two
actual handoffs around a reset (34), following and partial-corner visibility
(135 checks and 15 focused captures), Pool rooms/closet/duck (67), Pool annex
lights/towel (45), Office ordinary route (91), jump/railing safety (57), physical AI
ramp movement in an explicitly widened-home-bounds diagnostic (50), and shipped
AI home-bounds refusal (11). These small Static fixtures are collision/runtime
evidence; they do not replace final Full package visual, transition or performance
acceptance. [Native evidence](environment-collection-native-qa.md) and
[Office evidence](environment-collection-office.md) retain failed input recipes and
their corrected paths without disguising them as source fixes.

Native failures led to reusable fixes for NPC item/visibility reset, moved
interaction bounds, and admission of more than sixteen distinct dynamic meshes.
The renderer now supports the existing 256-object contract while preserving the
previous aggregate geometry bound. Essential CPU regressions cover ownership,
whole-assembly occlusion, real reset/movement/ray behavior and mesh admission.
The a9 preflight epoch passed formatting, strict workspace/all-target/all-feature
Clippy, locked production release build and all 181 Rust tests. Those receipts
precede the subsequently discovered capacity fix and do not validate its changes.
Asset validation
passes for 620 assets with zero warnings; texture validation retains 96 existing
advisories across 133 textures. Full-map baking remains explicit developer tooling
and is never part of Cargo tests or CI.

Capacity auditing found the eight previous standalone ZIPs total 479,914,576
compressed bytes but 1,466,020,347 decoded record bytes. The previous project
exporter incorrectly applied the ordinary 1 GiB aggregate bound to the entire
collection and retained completed maps' record vectors in memory. Its superseded
a9 run completed five maps before the audit paused it; an observed RSS point was
1,436,208 KiB. A macOS process sample reported a 5.0 GB peak physical footprint,
which is a different measure from RSS. Cooperative cancellation did not finish
within an extended grace period; the owned child was terminated, no package was
published, and its twelve exact source-snapshot files were checked against the
project before removal. Small receipts and logs remain preserved. Complete projects now have a separate bounded
2 GiB allowance admitted only after index/namespace validation, while every member
and shared-prop working budget retains 1 GiB and all typed/source limits remain.
Export streams exact records through owned disk spools with 64 KiB copy buffers,
deduplicates identical bytes and verifies size/hash on both copies. Six tiny
CPU regressions cover metadata budgets, transport, streaming integrity,
cancellation, atomic cleanup and deduplication without a map bake; all six pass,
alongside sixteen existing project/package-loading checks. Final formatting, strict all-target/all-feature Clippy, locked all-feature
release build and all 191 Rust tests now pass with unchanged Rust inputs across
the gates. The final player/compiler are preserved in `payload-collection-final`;
previous epochs remain intact. Final raw/compressed breakdowns are in `export-metrics-final.json`;
the exact compiler child peak RSS was 3,995,697,152 bytes. The copy buffer is not a
total-process memory bound.

The size investigation identifies props and lightmaps as 49.4% and 47.7% of the
previous eight ZIPs respectively. Exact whole-record deduplication finds no
remaining duplicates. A measured, bit-exact split of position, colour and UV
planes across prepared prop variants saves 53,448,181 compressed bytes across
Lantern, Winter and Arcade (11.1% of the old eight-map download), before small
replacement-recipe overhead. The corresponding raw reduction is 186,509,752
bytes. This lossless wire-format improvement is implemented with a
backward-compatible reader; original reconstructed record hashes, vertex float
bits, UVs and independent lighting settings remain unchanged. Three small codec regressions and one capability-admission regression pass,
including exact current PMP1 bytes, both recipe revisions, all accepted strides,
real chunk boundaries and malformed payload refusal. Older players reject v2 by
a named capability before fetching data; reused v1 records keep their original
requirement. These are baseline measurements, not the final collection size. Higher DEFLATE levels and another
lightmap tile codec offer much smaller measured download gains and do not justify
additional format changes here. A different compression library remains an
option with explicit compatibility and security costs, rather than an assumed
solution. Runtime decoded mesh and GPU atlas costs do not fall merely because the
download compresses better.

Runtime loads the selected member on demand from a file-backed ZIP. It keeps one
active simulated/rendered world, with a recent decoded-variant cache capped at
three entries / 192 MiB and separate optional CPU/GPU texture retention budgets
of 256 entries / 256 MiB each. Those limits do not cap the active world, incoming
world, total process RAM or GPU memory. During a door transition the old world
pauses while the destination is prepared/uploaded; commit replaces its simulation
and renderer resources. Last-owner release, shared assets, in-flight GPU work and
allocator/OS caching determine when memory falls. See `project.rs:127`,
`loading.rs:522`, `materials/image.rs:330`, `wgpu/texture.rs:968`, and
`game/app/mod.rs:3394`; the inactive-map corruption regression proves normal
selected loading does not read the other member's payloads.

Relay Annex, Beam, Closing Time, the approved R2 model, Spoonerman and historical
JSON sources remain preserved. Current-catalog compatible campaign packages were
exported and verified against their preserved compiler epoch, with byte-exact
source recovery and natural campaign/reset replays. Their exact binary epochs are
recorded rather than conflated with final collection evidence. See
[campaign compatibility](environment-0.12.0-compatible-campaigns.md).

Known automated geometry limits remain visible: Lantern Hollow and Beach exceed
the exhaustive room-check budget, Winter retains two room-leak candidates, and
intentional shared boundaries/openings remain diagnostic candidates in the hub
and Home. Native routes supplement these limits. No blanket suppression or per-placement
ignore was added; Office has one explicit annotation for its intentional railed
landing opening (289 geometry-intent records versus 288 in the imported
baselines).

Automatic approval review denied an optional native menu-policy disable/reopen
check as deferred menu-stage work, and denied a redundant nine-map Static bake.
Neither action executed or was retried indirectly. The enabled policy has native
evidence; separate narrow Home, Pool and Office collision fixtures were approved.
The exact actions, targets and review reasons are in `approval-blocks.json`.

The canonical local evidence root is
`local/environment-collection-0.12.0/`; `handoff.json` records source/package/binary
identities, inventories, acceptance paths, limitations and final delivery state.
