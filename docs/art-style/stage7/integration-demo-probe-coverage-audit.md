# Demo prepared probe coverage audit

[Exact machine witness](integration-demo-probe-coverage-audit.json). This audit
uses the preserved, superseded C1 Demo archive from
`debug-maps/art-style-hero/evidence/stage7-c1-interrupted/packages/`.
The archive is 85,226,656 bytes, SHA
`3a4561f1a909b1cc64b23fda1fb0c22c1f130da457ed72f583f09d35f3227acf`.
Its compiler fingerprint is
`4a7e5060b685495f0d046a68409521cb966ea8e1cf22e950e340cb64f2a1a550`.
No saved C1 measurement is relabelled as a final product. This specialist ran
bounded Python decoding and source inspection, then made the separately
allocated source correction below; no Cargo, compiler, bake or native job ran.

## Confirmed supported coverage gap

Demo rooms 7–10 form the authored enclosed Home loop, described as a 1.4 m clear
corridor with two blind turns. Room 7 is 1.7 × 16.85 m; room 10 is 4 × 1.7 m.
Both have a -0.9 m floor and 2.5 m height. The rooms contain authored practical
lights: fixtures 25–27 along room 7, 28 in room 8, 29–30 in room 9 and 31 in
room 10. They are accessible authored space, rather than decoration outside
navigation. The source file remains unchanged, SHA
`65833e98579a696167d616ce0cc94ebab85f744fdd72837ed247c483db1f9dec`.

C1 Medium and Full have identical PLPF v3 layouts and labels: origin
`[-7.819160461425781, -3.1218700408935547, -110.80005645751953]`,
2.0304698944091797 m cells, dimensions `[39, 8, 64]`, 19,968 slots and 1,397
valid probes. Rooms 7 and 10 have no labelled probe; room 8 has one, at its
far-right corner. The final-label rejection uses real air volume, walkable
floor height, ceiling height, 5 cm wall/solid clearance and opaque geometry.

The runtime accepts valid probes only when squared lattice distance is strictly
less than four, then verifies the stored owner and compiled-solid visibility.
Its radius is 4.060939788818359 m. At a saved walkable rat-cell centre in room 7,
`[60.900001525878906, -0.8199999928474426, -5.5]`, the nearest valid probe is
9.47658259996179 m away in room 6. There are **zero candidates before visibility**.
Thus no doorway, neighbour or disconnected-room rule can provide a sample there.
The same room-8 witness has zero candidates, with its nearest probe 4.19434438 m
away. Counts below reproduce the sampler's f64 lattice-distance predicate at
each saved walkable cell's half-body height, before visibility filtering:

| Room | Reference humanoid walkable cells | No radius candidate | Rat walkable cells | No radius candidate |
| --- | ---: | ---: | ---: | ---: |
| 7 | 336 | 275 | 504 | 414 |
| 8 | 176 | 92 | 264 | 139 |
| 9 | 360 | 0 | 540 | 0 |
| 10 | 80 | 0 | 120 | 0 |

All listed room-7/8 humanoid cells share region 0 with the Home rat-spawn area's
humanoid cell. Their rat cells share region 3 with that spawn. This establishes
saved navigation connectivity, not an observed AI visit or a native traversal
measurement. Positive candidate counts do not establish visibility: room 10's
ownership warning alone therefore cannot prove unresolved lighting. Its six
raw candidates at the chosen witness include adjoining rooms and still require
normal wall/opening tests.

`entity_lighting` reports `Unresolved` and uses the existing authored
`lighting.sample` fallback at a supported position with no sample.
`entity_spatial_lighting_with_visibility` returns no spatial payload when none
of the centre/bounds anchors has static support; shader shading then uses the
historical environment scale. A compact rat model at the room-7 witness is much
smaller than the five-metre support deficit, so its bounds anchors cannot
recover the missing field. This is a loss of prepared transport response on a
supported route. It is not a measured black/fullbright or through-wall leak
claim. The fallback retains static authored lighting but does not carry the
prepared diffuse moments or spatial live-source visibility payload.

## Root cause and allocated correction

`probe_lattice` uses the global chart-receiver bounds, selects one cubic spacing
`max(1.5, extent_per_axis/64)`, and centres each axis. The long world forces
2.030 m cells. Room 7's nearby X sites are 60.201584 and 62.232048 m, outside
its clear corridor X range of approximately 60.35–61.65 m. Room 10's nearby Z
sites are 3.921494 and 5.951965 m, outside its clear Z range of approximately
4.45–5.75 m. Room 8's sole valid corner does not support its full length.
Neither source defaults nor invalid owner labels cause this gap.

The primary stopped its jobs and authorized a bounded generic phase correction.
The former compiler `placement_at` predicate was moved unchanged into
`lighting::probe_placement`; phase scoring and final compiler labelling share it.
The centred grid remains bit-identical when every room has valid support.
Otherwise the compiler checks at most 63 additional XYZ quarter-cell phases,
using `[0, -0.25, 0.25, -0.5] × cell_m` on each axis. The initial coverage scan
can stop strict checks once each room is represented. If a room is missing, each
additional candidate counts **all** strictly valid probes. Selection first
maximizes covered-room count, then compares sorted ascending per-room counts
lexicographically, balancing the least represented rooms. A change must recover
a new room and retain every originally covered room. Ties retain the earlier
phase; infeasible rooms remain uncovered and retain warnings. All 63 candidates
are considered; finding any probe in every room does not end the search.

`TransportScene` owns the original and selected layout. Target preparation and
probe baking use the selected positions, and baking checks the original chart
layout plus exact target-position bits. PLPF v3, axis/total caps, interpolation
radius, opaque visibility, target energy/room semantics, invalid-slot behaviour
and aligned zero-source direct sidecars remain unchanged. Solver revision 16
invalidates earlier physical caches. No authored room, light, asset, camera,
collision volume, brightness threshold or runtime radius was changed.

Owned source files are `src/lighting.rs`, the new
`src/lighting/probe_placement.rs`, `src/compiler/probes.rs`,
`src/lighting/transport.rs`, `src/lighting/transport/tests/probes.rs` and
`src/render/common/light_transport.rs`. Source is frozen and released to the
primary for formatting and serialized validation. Canonical current revision
documentation is owned by the primary; dated Stage 1–6 evidence remains unchanged.

## Required acceptance evidence

Six focused regressions cover original-phase/byte preservation, no coverage
tradeoffs or fabricated blocked-room recovery, exact target/bake/PLPF v3
alignment and zero-source sidecars, authored wall/floor/closed-prop rejection,
the actual Demo source corridor, and interior population after an earlier
boundary-only phase. The Demo test uses actual source
architecture/material triangles with asset-less prop placeholders as additional
solids; its `PLACES_VERBOSE=1` log records selected origin, retained baseline
coverage and all room counts. It passed in the primary
[focused-v2 receipt](execution/probe-focused-v2.json): all 21 probe tests passed,
zero failed/ignored, 15.00 s test time and 44.54 s including build. Selected
origin is `[-8.834395, -3.6294875, -111.30767]`, with strict room counts
`[15,15,10,174,14,36,77,16,8,18,4,16,1592,12]`; rooms 7/10 now have 16/4
valid probes, and every originally covered room remains represented. The
unchanged room-7 runtime support assertion passed with source geometry and
normal compiled visibility. Existing enclosed/dark/disconnected-space tests
also passed in this focused suite. This is not final resolved-package evidence.

The retained [preflight-v7 log](execution/rust-preflight-v7.log) shows why the
first phase strategy was inadequate: strict format/check/debug+release Clippy
passed, but two of 20 probe tests failed. Its first all-labelled-room phase
provided only one boundary probe in room 7 and failed the unchanged runtime
witness. The revised count balancing evaluates every bounded phase; the new
regression distinguishes `[4,1]` boundary recovery from `[4,4]` interior support.
The other failure exposed a floor-only synthetic chart without real vertical
air extent and an interval that already contained the original X site after
adding the ceiling. The fixture now includes floor and ceiling, uses the truly
missed interval 2.45–2.95 m and explicitly asserts original coverage
`[true,false]`. Target/bake/sidecar and blocked-control assertions remain intact.

The primary [strict preflight-v8](execution/rust-preflight-v8.json) passed
format, workspace/all-target/all-feature check and strict debug/release Clippy
in 78.37 s. Focused compiler probes, roof spans and geometry suites passed
5/5, 16/16 and 28/28. The primary
[normal release checkpoint](execution/release-compiler-c1-v3.json) also passed
`cargo build --locked --release` in 96.05 s. These are retained source-checkpoint
receipts. The primary separately owns strengthening an obsolete Demo geometry
fixture assertion and its later source gate; full workspace tests and final
normal products remain required. Final acceptance also requires independently
decoding the **fully resolved-asset** Demo field, confirming local support along
rooms 7/8/10 using normal visibility, retaining originally covered hero
phase/output, and capturing native moving-entity transitions. Implemented source
and a placeholder-geometry test do not certify final saved-field or native
coherence. Those final gates remain pending in this report.
