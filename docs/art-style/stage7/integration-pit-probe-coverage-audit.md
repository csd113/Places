# Pit lower-floor prepared probe audit

[Exact machine witness](integration-pit-probe-coverage-audit.json). The audit
uses preserved, superseded C1 `levels__level0_pit.placesmap`, SHA
`466f4fe029f3774870c4aa6fe418721c93bbdd41e0cd4e8740ac0136e0a9fec6`.
The current local source remains unchanged, SHA
`fe67c23ddc9742fc819befdecc38d1dbce5000c281d4c7eec06ce8367989db38`.
Its room 16–24 fields match the saved prepared semantics after f32 parsing.
This is a read-only archive/source/segment audit; no source, asset, Cargo,
compiler, bake or native job was changed or run by this specialist.

## Zero labels are overlapping ownership

Rooms 16–19 are real lower-floor authored space: floor -7.6 m, height 2.2 m,
areas approximately 149.445, 149.445, 73.8 and 73.8 m². The primary's independent
[navigation witness](validation/c1-pit-navigation.json) establishes actual
lower-floor humanoid walkable cells in navigation region 8. They are not
classified as decoration or outside the field.

Those lower air volumes overlap tall shaft rooms 20–23, with floor -11 m,
ceiling 6 m and smaller areas 144, 144, 72 and 72 m². The existing
`LevelLighting::indexed_room` rule selects the smallest containing room area,
retaining the earlier room on equal areas. Every tested lower-floor cell's
actual air owner is the corresponding shaft room. The lower room's missing
label therefore does not imply an unsupported sample. Shaft labels 20–23 have
555, 555, 286 and 288 valid probes throughout their actual volumes.

The saved Full PLPF v3 origin is
`[5.250011444091797, -18.749980926513672, -33.75001525878906]`, with 1.5 m cells,
dimensions `[49,17,31]`, 25,823 slots and unchanged 3 m runtime radius. The
irradiance blob SHA is
`3bc87ad31afc173f01bf61bd3fc3f573f540b0c675cce2af3871a135966ae598`.

## Saved compiled visibility witness

The bounded decoder checked declared byte lengths and SHA hashes, consumed
every byte of PLPF v3, PLNV v1 and PLLT v2, and used the **saved lighting
visibility solids** rather than equating lighting with collision or assuming
positive radius counts imply support. PLLT contains 25 room volumes, 114 local
lights, 228 wall boxes, 927 horizontal slabs/interfaces and 409 prop boxes.

Every saved lower-floor walkable humanoid cell was queried at half-body height,
approximately -6.7 m. The query applies the sampler's f64 lattice-distance
predicate, checks the stored probe owner, requires valid actual-air/zone
lookups, and tests clear segments against all relevant compiled solids. The
independent segment calculation retains the existing f32/FMA start nudge,
strict box overlap, floor contact/crossing and oriented prop transform. It does
not depend on reconstructing the runtime spatial indices. The height lookup's
historical footprint fallback is represented separately from the strict air
requirement in `probe_region_at`; fallback alone does not establish support.

| Authored lower room | Walkable lower-floor cells | Runtime air owner | No radius candidate | No clear, label-valid candidate | Minimum clear candidates |
| --- | ---: | ---: | ---: | ---: | ---: |
| 16 | 350 | 20 | 0 | 0 | 14 |
| 17 | 350 | 21 | 0 | 0 | 12 |
| 18 | 268 | 22 | 0 | 0 | 12 |
| 19 | 268 | 23 | 0 | 0 | 14 |

All 1,236 samples have local prepared support under that independent saved-solid
calculation. Each has at least 29 raw candidates, but only the clear, label-valid
ones are counted as support. For example, room 16's minimum-support witness
cell 19,648 at `[55.500003814697266,-6.699999809265137,-22.900001525878906]`
has 34 raw candidates and 14 clear candidates. Its first clear probe is label
20, ID 6,256 at
`[55.5000114440918,-5.999980926513672,-22.500015258789062]`, distance
0.8062353565351029 m. Exact per-room minimum witnesses are retained in JSON.

No geometry migration, owner-rule change, fabricated room recovery, radius
extension or source edit is justified by these zero labels. The bounded solver-16
phase selector is permitted to retain a layout when no additional strictly valid
room ownership is feasible. This audit does not certify the whole map's other
floor layers or every native entity/source state. The primary still verifies
the final normal rebuilt package with the actual Rust sampler and native
transition checks; saved C1 evidence is not relabelled as final acceptance.
