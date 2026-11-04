# Changelog version and evidence audit — 2026-11-04

This documentation audit reconciles the main changelog with Git history, local
tag targets, the live GitHub tag collection, all six published GitHub Releases,
historical changelogs and the consolidated release reports. It does not create
or rename releases, tags or branches, and it does not rerun historical native
or performance campaigns.

## Coverage and date convention

The live repository has six version tags: `0.7.0`, `0.8.0`, `0.10.0`, `0.11.0`,
`v0.12.0` and `v0.13.0`. All six have published GitHub prereleases and now have
feature/fix/performance summaries. GitHub publication dates are used for these
headings. In particular, 0.7.0's source baseline is September 23 but publication
is September 24; 0.12.0's source finalization is October 18 but publication is
October 19. Other dates describe the source history, not a verified publication.

The versioned 0.9.0 preparation is retained explicitly: its report says no tag
or GitHub Release was created, and neither appears in the live collections.
No empty 0.9.0 release is invented to close the numeric gap.

The inherited changelog also records the first App Manager-ready 0.1.0 through
0.6.0 and intermediate patch milestones. Their highlights are retained even
where commits combine multiple stages. A documented stage is not treated as
proof of a separately published release.

| Main Places version | Primary source boundary/evidence | Attribution limit |
| --- | --- | --- |
| 0.13.0 | [v0.13.0 / b2436175](https://github.com/csd113/Places/tree/v0.13.0), [release](https://github.com/csd113/Places/releases/tag/v0.13.0), [report](release-0.13.0.md) | Summarize accepted implementation; preserve rejected candidates, regressions and platform limits in the report. |
| 0.12.0 | [v0.12.0](https://github.com/csd113/Places/tree/v0.12.0), [release](https://github.com/csd113/Places/releases/tag/v0.12.0), [report](release-0.12.0.md) | No general gameplay checkpoint feature; reported defects remain defects at this boundary. |
| 0.11.0 | [0.11.0 / d5942839](https://github.com/csd113/Places/tree/0.11.0), [release](https://github.com/csd113/Places/releases/tag/0.11.0), [report](release-0.11.0.md) | Do not include later native editor/Luau work merely because it initially retained package version 0.11.0. |
| 0.10.0 | [0.10.0 / c2a3a992](https://github.com/csd113/Places/tree/0.10.0), [release](https://github.com/csd113/Places/releases/tag/0.10.0), [report](release-0.10.0.md) | The source commit is October 15; publication is October 16. |
| 0.9.0 | [5b934038](https://github.com/csd113/Places/commit/5b9340381c74), [report](release-0.9.0.md) | Source preparation only; no matching tag or GitHub Release. |
| 0.8.0 | [0.8.0 / e1bd1e84](https://github.com/csd113/Places/tree/0.8.0), [release](https://github.com/csd113/Places/releases/tag/0.8.0), [snapshot notes](https://github.com/csd113/Places/blob/0.8.0/CHANGELOG.md) | Includes work after the pre-wgpu tag, even while Cargo still said 0.7.0. |
| 0.7.0 | [0.7.0 / 4ad6d97d](https://github.com/csd113/Places/tree/0.7.0), [release](https://github.com/csd113/Places/releases/tag/0.7.0), [snapshot notes](https://github.com/csd113/Places/blob/0.7.0/CHANGELOG.md) | Release itself freezes the baseline; feature bullets consolidate its already-integrated changes since 0.6.0. |
| 0.6.0 | [a062cf3b](https://github.com/csd113/Places/commit/a062cf3b5d2e) and inherited notes | No matching publication/tag in current collections. |
| 0.5.3 | [a6e7977e](https://github.com/csd113/Places/commit/a6e7977e) and inherited notes | Notes say September 21; implementation is September 22 and Cargo retains 0.5.2. No separate package bump/tag verified. |
| 0.5.2 | [85895b55](https://github.com/csd113/Places/commit/85895b559b21) | Pool/Office content stage; no matching publication/tag. |
| 0.5.1 | [f9306124](https://github.com/csd113/Places/commit/f9306124c4c5) | PNG material stage; no matching publication/tag. |
| 0.5.0 | Geometry milestone in the [inherited notes](https://github.com/csd113/Places/blob/3fee560e484d5dbc61496b34c90cdef8292dbe2b/CHANGELOG.md#050--2026-09-21) | Root Cargo history jumps from 0.4.0 to 0.5.1. No distinct post-reset 0.5.0 bump/tag verified. |
| 0.4.0 | [d8b41708](https://github.com/csd113/Places/commit/d8b4170803b1) | Catalog/environment/entity organization stage. |
| 0.3.2 | [d4011237](https://github.com/csd113/Places/commit/d4011237d631) | Desktop controls stage. |
| 0.3.1 | [1932dbb5](https://github.com/csd113/Places/commit/1932dbb55902), followed by the move to repository root | Standalone workspace/package migration. |
| 0.3.0 | [13680b68](https://github.com/csd113/Places/commit/13680b68e456) | RGB lighting and decal work are combined in the imported history. |
| 0.2.0 | RGB milestone in the [inherited notes](https://github.com/csd113/Places/blob/3fee560e484d5dbc61496b34c90cdef8292dbe2b/CHANGELOG.md#020--2026-09-21), combined at 13680b68 | No distinct post-reset 0.2.0 package bump/tag verified. |
| 0.1.0 | [0f92f4d1](https://github.com/csd113/Places/commit/0f92f4d1ef31), followed by packaging finalization | App Manager-ready numbering reset; independent GitHub publication unverified. |

## Correcting misleading historical labels

The former root file had one actual 0.13.1 Unreleased block and many older
unversioned stage logs still called Unreleased. Their position alone was not a
release boundary. For example, movement/water/entity entries appeared below
0.7.0 but were implemented after its tag and belong to the 0.8.0 generation.
Likewise a later SDL3 bullet was inserted into the old Settings section even
though the original 0.7.0 tag still used SDL2/OpenGL.

The new summaries use actual tag ancestry and historical snapshots:

- 0.6.0 → 0.7.0: quality/material/lightmap/rendering/settings foundations,
  lighting isolation and depth repairs already present in the GLES2 baseline.
- 0.7.0 → 0.8.0: wgpu/SDL3, offline packages, movement/water/animation,
  interactions/entities, final architecture, outdoor/Halloween content and
  lighting/compiler hardening.
- 0.8.0 → 0.9.0 preparation: eight environments, linear HDR/art integration,
  menu/storage/map-independence work, terrain recovery and crash diagnostics.
- Later releases use the published tag boundary plus their release summaries;
  consolidated report filenames often describe the package version at the
  beginning of development, not the release that eventually contains it.

Detailed prior notes remain available in the immutable
[pre-audit changelog](https://github.com/csd113/Places/blob/3fee560e484d5dbc61496b34c90cdef8292dbe2b/CHANGELOG.md).
The highlights omit minor fixes and stage/test/deletion accounting; no evidence
is removed from Git history or the release reports.

## Independent numbering and evidence gaps

The earliest prototype notes run from 0.1.0 through 0.8.0 on September 20–21,
before the reset to the first App Manager-ready 0.1.0. These are summarized in
their own table, including the documented 0.3.1 appearance-only iteration.
Some iterations have no individual source commit/version bump in the imported
history. Their documentary labels are retained without a publication claim.

The [PocketCHIP App Center mirror](https://github.com/csd113/Places/blob/b208b4647d20/apps/places-pocketchip/CHANGELOG.md)
has another independent 0.8.0–0.11.1 series dated September 25–26. It was imported
at `b208b464` and later removed from the desktop checkout. Its ARMv7 packaging,
GPU-fault repair, shader/batch/texture/atlas/culling optimizations and final patch
are summarized separately. Its device-specific measurements are historical
reports, not newly repeated desktop measurements or proof that later wgpu
releases ran on PocketCHIP.

The local `renderer-gles2-reference` tag is a renderer checkpoint, not another
semantic release; it is absent from the live tag list at audit time and receives
no invented version entry. No missing 0.13.1 release is inferred: development is
still Unreleased.

## Preservation and validation

The active 0.13.1 block is preserved verbatim from the working branch, including
the hub task's verified **32 distinct authored movable props/NPCs per project
session** limit, visited-level accounting, player exclusion and unsupported cases.
This audit changes released/historical summaries and adds this evidence report;
it changes no game, editor, compiler, map or asset implementation.

Validation checks cover every semantic version heading in the previous root
changelog, all six published release/tag pairs, unique main-series headings,
the complete prototype/mirror tables, preserved Unreleased content, existing
local Markdown destinations and patch whitespace. Native builds and performance
tests are not applicable to this documentation-only change.
