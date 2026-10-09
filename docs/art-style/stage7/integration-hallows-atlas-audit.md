# Hollow atlas capacity audit

The preserved successful entry package has Medium six pages and Full eight pages. C1-v4 retains a valid eight-page Medium atlas but Full reports `page overflow` and has neither atlas nor irradiance. Its build/currentness/decode case passed; that receipt does not establish visual compatibility.

The normal gate stopped at 2026-10-09 00:03:25 UTC. C1-v4 packages and source freeze v4 are superseded evidence. The all-map campaign, recovered weather comparison and native acceptance remain pending.

The source remains byte-identical: 348,530 bytes, SHA `1e2c53846cfa21892b3b66e43658925ec28fcea8f4143324f998e1193a91d0c0`. All 243,202 prop receiver patches remain exact ordered equals. Every current Medium prop chart is precisely the entry width plus one and height plus one. Former one-sample prop-axis charts decrease from 179,308 to zero. The endpoint contract must remain intact.

| Profile | Charts | Pages | Data texels | Padded reservations |
| --- | ---: | ---: | ---: | ---: |
| Entry medium | 245,447 | 6 | 2,227,632 | 5,160,314 |
| Entry full | 245,447 | 8 | 3,680,557 | 6,982,381 |
| C1-v4 medium | 245,495 | 8 | 3,504,615 | 7,450,041 |

Model charts reserve one texel per side; architecture retains two. The existing allocator already reuses architecture holes when model pages reach the shared cap. Chart rotation is forbidden by the frozen physical UV orientation. Raising sampling endpoints exposes real added allocation demand; deleting endpoints, gutters or charted content would violate maintained contracts.

Actual medium planning: 245,495 charts reserve 7,450,041 texels. Their mathematical area lower bound is 8 pages; the unchanged deterministic packer first succeeds with 8 pages. Base GPU half-float storage is 134,217,728 bytes and CPU float storage is 268,435,456 bytes. Planning and replay took 22.543 seconds.

Actual full planning: 245,495 charts reserve 9,549,535 texels. Their mathematical area lower bound is 10 pages; the unchanged deterministic packer first succeeds with 10 pages. Base GPU half-float storage is 167,772,160 bytes and CPU float storage is 335,544,320 bytes. Planning and replay took 47.128 seconds.

The diagnostic changes only its planning limit to the existing decoder safety bound 64, allocates no atlas pixel buffers and invokes no fill. It replays exact rectangle dimensions at every budget from the production cap through first success and validates every occupied chart and gutter with a linear pixel occupancy witness. The existing strict bundled test remains unchanged.

The eight-page policy is distinct from codec and GPU safety limits. Increasing it requires a measured per-group memory/layer proof, preserved package byte bounds, strict successful Full allocation and final normal-renderer validation. JSON retains hashes, every per-kind count, complete dimension histograms and per-budget placement receipts.

Evidence: [JSON](integration-hallows-atlas-audit.json), `src/static_prop_lighting_tests.rs::lantern_hollow_atlas_capacity_profile`, the preserved C1-v4 case receipt and entry archive paths recorded in JSON.

The measured Full reservation exceeds nine-page capacity by 112,351 texels. Ten pages require 20 array layers and 160 MiB for the base HDR texture, with 320 MiB of CPU float texels during preparation. The pinned wgpu 30 device defaults provide 256 array layers and an 8192px 2D edge; this renderer changes only bind groups to 5. Hollow has no switch groups, so its encoded HDR payload is 160 MiB and remains below the existing 256 MiB + 64 KiB record guard. The existing four-group limit and group-dependent byte guard must remain intact. Capacity does not imply permission to exceed that payload bound.

The minimal measured remedy is a ten-page Full policy and matching renderer capacity, with existing lower-profile eight-page budgets retained where practical. No endpoint, density, gutter, orientation or content reduction is justified; no allocator redesign can fit the reservation in eight or nine pages. The bounded production implementation now uses Full ten pages, Low/Medium eight pages and a renderer alias of the shared maximum. Its serialized source and final package/native checks are pending. The profile receipt records exit0, one strict diagnostic pass and no atlas fill.

Changed source files are `src/lighting/lightmap/mod.rs`, `src/lighting/lightmap/tests.rs`, `src/render/wgpu/lightmap.rs`, `src/quality.rs`, `src/quality/tests.rs`, and `src/static_prop_lighting_tests.rs`. Source comments now state two RGBA16F layers and actual resident allocation. The page policy changes no chart dimensions, density, gutters, LOD, packing, source content or shader. Existing config-key hashing includes `max_pages`; focused controls distinguish the old Full eight-page key from the ten-page key while retaining the exact Medium key. Geometry 7, solver 16 and all wire layouts remain unchanged.

The historical three-storey tower remains a strict explicit eight-page success with a four-page failure. The existing real bundled Medium/Full planning test is unchanged. The artifact-backed dense Hollow test remains strict for its measured ten-page Full atlas and requires a freshly compiled package, because superseded C1-v4 contains the demonstrated failed Full variant. No validation fallback is accepted.

## Fresh package correction observed — 2026-10-09 UTC

The source gate in [preflight v13](execution/rust-source-preflight-v13.json)
passed format/check, strict debug and release Clippy, the explicit cache policy
control, 13 renderer lightmap controls, 29 quality controls and the unchanged
real bundled-map planning assertion. The frozen normal compiler is
`b6188e665f9f6b8e8657b08299890afd66f5ee8c108bb0329233756c97c869ae`.

The ongoing ordinary v4 campaign's Hallows case completed its build, required-
current verification and decode validation with exit 0. The build ran from
00:33:00.184348 UTC for 359.773 seconds; subsequent verification began at
00:38:59.958857 UTC. Its Medium atlas
has eight pages; Full has ten; both contain 245,495 charts, report no lightmap
failure and retain their prepared irradiance record. This restores the actual
resources lost by the superseded v3 campaign. The independent
[first-six original-availability check](validation/availability-v4-first-six.json)
reports zero losses, with 42 supported cases pending. Final campaign and native
acceptance remain pending; restored resources do not establish gameplay FPS.
