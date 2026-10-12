# Arcade restroom validation — 2026-10-12

A 6.6 × 7.8 m restroom is connected through the existing Arcade west wall. The bowling, arcade and accepted birthday rooms retain their original bounds, contents and walking routes. All 172 protected files, including Bepsi, party art, Office, Spooner-Man and pre-existing user audits, were hash-checked unchanged. The existing Cosmic switch now addresses its four existing fixtures as one circuit; their positions, brightness and UV material behavior are unchanged.

The room has three stalls with real porcelain toilets, a wider first bay and grab bar, two wall-mounted basins with taps and soap, framed mirrors, 30 cm floor tiles, 20 cm wall tiles, ceramic skirting, and two ceiling ventilation grilles. The 1.4 m entry and 1.165–1.315 m stall leaves are interactive and initially closed. The commercial door kind uses the established interaction, swing and obstruction handling with thick flat laminate leaves. New-map guidance specifies closed doors by default; existing maps were not mass-edited.

## Reusable assets and corrections

The editable Blender source and ten GLBs live in `assets/environments/arcade/restroom/`; the complete kit is 248,832 bytes and 1,708 unique triangles. Three purposeful Blender passes reviewed porcelain silhouettes, basin hardware and ledge seams, partition feet/headrails, closed frame topology, UV swatch placement and export bounds. All ten meshes have zero degenerate triangles, boundary edges, nonmanifold edges or flipped faces. The final native refinement made the playable door leaves honor the teal catalog tint and raised the grab bar above the tank. Existing committed PNGs supply every surface; no texture generation or mirror-specific renderer was added.

The sink wall exposed an engine UV defect: opening fragments inherited their own top as the vertical origin. Wall faces now retain the parent wall/ceiling vertical reference through sill, header and cross-wall fragments. Small X/Z and explicit/ceiling-bound mesh witnesses protect the continuous grout phase; geometry cache revision 10 invalidates stale geometry without changing the application or package version.

## Lighting and mirrors

Four neutral troffers use one reachable physical wall switch. The interaction changes actual source illumination and fixture emission. Full lighting sums the members into one prepared circuit contribution; the four existing Cosmic sources form a second circuit. Eight physical switchable sources remain within the existing limit, with two circuit contributions plus the base contribution. Static lighting stores one optional pre-lit off geometry/prop state built from the same already-baked lighting, sharing PNGs. Runtime swaps prepared banks and preserves source states across lighting-mode changes. No per-click bake, enlarged contribution limit, shader layout change or new per-frame framework is introduced.

The two coplanar mirror panes use the existing Full planar reflection path. Static lighting or Reflections Off leaves an ordinary nonreflecting pane. This is the existing renderer limitation, not a painted substitute advertised as a functioning mirror.

## Validation

Formatting, strict workspace/all-target/all-feature Clippy, default Rust tests (2,237 library plus eight integration tests), and all-feature Rust tests (2,243 library plus eight integration tests) passed. Asset validation covered 568 entries with no warnings; the broader model/PNG audit and existing authoring freshness checks passed. The source Python suite passed, followed by affected source/archive fixture checks. No default Rust test or CI job bakes a shipped map.

Geometry validation reports zero errors and 34 pre-existing duplicate-anchor warnings for accepted party balloons. Those anchors and animations remain unchanged. Small regression fixtures cover circuit membership and energy, repeated source toggles, Static off payload round-trip/reuse and malformed alternate records, and the visible restroom door tint.

Native wgpu/Metal inspection on Apple M2 Pro at 1280 × 720 confirmed aligned grout, grounded fixtures, clear grab-bar/tank separation, real reflected stall geometry, and no GPU validation/model-loading failures. Four selected tiny native smoke checks passed: nonempty upload, texture identities, material passes, and world replacement/repeat with cached 1024px sheets and normal map.

Held-control walkthroughs first stopped at the closed entry (`x=0.201`) and first stall (`x=-3.699`), then used E to open them, entered to the rear wall (`x=-6.3`) and returned to Arcade. The other two stalls were likewise opened, entered and exited. The basin wall stopped the player at `z=7.5`; jumping stopped the eye at `y=2.9961` under the 3.2 m ceiling. Partition jumps remained blocked at `x=-3.681`. All five routes returned to a stable `y=1.6` floor with no void fall or escape.

Four physical switch presses produced on/off/on/off/on in each lighting mode. Repeated on and off images are pixel-identical; mean RGB on/off differences are `[98.20, 101.12, 97.42]` in Full and `[33.81, 35.31, 35.70]` in Static. Full-off → Static-off → Full-off retains an identical Full-off image. Static preserves its existing ambient fill. Its alternate bank adds 14,105,168 vertex bytes and 390,192 index bytes, sharing images; Full allocates no alternate bank.

The final ignored Arcade package is **43,897,722 bytes** (41.86 MiB), versus 45,317,178 at handoff: −1,419,456 bytes (−3.13%). SHA-256: `6df925a0bc140e6bb448477c7e1d2ed900f009de019693a23a39533e834e9510`. `verify --require-current` matched fingerprint `aa8e7befbb52674c4c18ad6f4cecc677c711e24e6330b3cb1d1cdd720385da32`; validation found 21 entries, 96 intact dependencies and no warnings. Full has five 1024-edge atlas pages, three contribution groups and 30 storage layers: **240 MiB**, versus 320 MiB at handoff (−25%).

Two sequential 1,200-ready-frame stationary sink runs used High textures/filtering, Full lighting, vsync disabled and the existing GPU-completion drain. The final 240 genuine-world samples are reported below. These are CPU frame/completion timings, not GPU timestamps. The mirror adds approximately 1.13 ms at the median in this bounded comparison; the p95 variation is noisy. There is no reliable clean pre-restroom frame baseline, so no whole-map FPS improvement is claimed.

| Reflection setting | Frame median / p95 (ms) | Render median / p95 (ms) | Main draws | Planar passes |
| --- | ---: | ---: | ---: | ---: |
| Full | 6.29 / 15.59 | 4.77 / 14.29 | 159 | 1 |
| Off | 5.16 / 18.79 | 3.74 / 17.43 | 159 | 0 |

Reproduce the package gate with `target/release/places-compile build levels/retro_arcade_demo.json --out assets/levels/retro_arcade_demo.placesmap --workers 8` using native Metal access, followed by `verify --require-current` and `validate`. Camera/settings witnesses are in `tests/fixtures/native/arcade-restroom-manifest.json`; `tools/bench/capture_art_style_hero.py` performs the explicit native captures. The final successful normal build took 369,831 ms.

## Preservation and follow-through

Only `retro_arcade_demo.placesmap` was compiled with normal reuse and eight workers. All nine other pre-existing package files remained byte-identical during validation. Seven other active maps will need a normal compile after their queued edits because the catalog, tool identity and geometry revision changed; no unrelated library bake was performed here. The queued fluorescent tint, prop-size warnings, Medium filtering, snow distance and Office repairs remain with their respective owners.

The historical sources were moved byte for byte outside active discovery, build and level validation:

| Source | Bytes | SHA-256 |
| --- | ---: | --- |
| `docs/archive/levels/places_demo.json` | 537,437 | `65833e98579a696167d616ce0cc94ebab85f744fdd72837ed247c483db1f9dec` |
| `docs/archive/levels/model_zoo.json` | 313,154 | `71ff7d364cd8620f6800c0ab0bb65a41a43226b1579ec6ff8e9fa29477aedfd7` |

Their disposable ignored packages total 154,436,171 bytes. At handoff they matched those source hashes, catalog `391254da90c8236cff5901ddef75c93545ec14a7786c472e23412cd3eec8ee75`, tool hash `b77f987ebbb76f104f80cdb18581acc8516066bf6ee7048ef0cd25c80bb43380`, probe capture enabled, and commit `f9c8eed6965b1c5664891fc242a8f8e2277ccce0`. They are removed after publication validation; neither archived map was baked during this task. Only preservation hashes remain in default tests. The optional generator requires an explicit destination.

## Final screenshots

[Arcade entrance](../images/arcade-restroom/entrance.png) · [Room and closed stalls](../images/arcade-restroom/room.png) · [Sinks and working mirrors](../images/arcade-restroom/sinks.png) · [Toilet and grab bar](../images/arcade-restroom/stall.png) · [Ceiling vents](../images/arcade-restroom/ceiling.png) · [Static off](../images/arcade-restroom/lights-off-static.png) · [Full off](../images/arcade-restroom/lights-off-full.png)

Seven unaltered native PNGs total 2,907,763 bytes. The reusable kit and report remain small source deliverables; the 41.86 MiB compiled package stays ignored in Git.

Publication cleanup removes the build target, disposable task logs and intermediate captures after exact-SHA CI completes. It retains original inputs, the reusable kit, the current ignored Arcade package, these final screenshots and this report. Actual cleanup totals, publication SHA/CI and custody release are recorded in the delivery message.
