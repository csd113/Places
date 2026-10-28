# 0.13.1 editor acceptance (unreleased)

The first phase added rectangular/circular pillars and fitted automatic trim.
Its accepted source is `fa9a8b93cbfba0b0e8a1b1ee064c01cbbe1be9e1`; its receipt,
native captures and binaries remain under `local/verification-0.13.1-trim/`.
The [changelog](../../CHANGELOG.md) retains its exact acceptance scope.

The second phase exposes New Room, Add Wall, Add Door, Add Light and Play in Simple
mode. Make Doorway cuts an aperture; Add Door creates or fits a functional leaf.
Wall placement has a private preview, optional chaining, snapping and cancellation.
Body dragging and endpoint handles preserve openings, doors, wall lights/decals
and automatic trim in one history transaction. Floor decals stay fixed. Existing
room stretching retains attachment, shared-boundary and elevated-prop guards.
Four visibility groups, room/wall isolation and persistent object locks support
focused editing. Material browsing has 24 compact tiles in four rows or 15 larger
tiles in three, while retaining labels, selection, search, theme and paging.
Eleven absent material previews are committed opaque 160x120 PNGs; existing
previews and source textures are unchanged.

## Razer acceptance, 2026-10-28

Formatting, strict workspace/all-target/all-feature Clippy, the locked release
workspace/all-feature build, 367 Rust tests and 369 Python source tests pass
(18 expected source skips). Two small CPU regressions protect atomic wall
attachments/source/history/focus identity and DPI control/hit correspondence;
ordinary tests never bake production maps. The asset validator checks 644 assets
with zero warnings. Texture/prop checks pass with their existing manifest and
preferred-budget advisories.

Native SDL verification uses ordinary editor handlers, real windows/GPU rendering,
framebuffer captures and source checkpoints. The accepted recipes cover adjacent
room creation; four chained walls; doorway versus leaf; light and materials;
repeat/cancel; locks, isolation and hidden layers; wall body movement, both
endpoints, aperture-conflict refusal, atomic undo/redo; and room stretching with a
door and trim. Save/reopen retains source. Simple Play saves/exports/builds,
launches the game and returns cleanly; native captures show the new door closed
and opened, and the player trace crosses its opening. Failed exploratory recipes
are retained separately and excluded from acceptance assertions.

Visual layout checks cover 960x544, 1280x720/800, 1000x900 and 1440x900 on the Razer
at Windows 150% display scale. CPU coordinate checks also cover 100/125/200% and
1x/2x drawable density. Actual Mac Retina validation is deferred. Fifo presentation
was negotiated; these checks make no uncapped FPS claim.

## Boundaries and retained evidence

Walls remain axis-aligned and must fit one room floor. Connected perimeter
changes use room edge stretching; edits that sever junctions, cut through openings,
cross shared-room boundaries or ambiguously move elevated fixed props are refused
with feedback. Visibility groups and isolation are session state; object locks
persist. Custom layer authoring is outside this bounded implementation. Advanced
numeric/build tools remain available, while Simple Play builds automatically.

`local/verification-0.13.1-editor/validation-receipt.json` records the accepted
commit, hashes, cleanup and remote verification. `accepted-bin/` retains the
production executables and DLLs; `accepted-project-v3/` and
`accepted-project-release/` retain small sources/packages. Recipes, logs,
`assertion-receipt.json` and native captures provide the supporting evidence.
No production map rebake, main merge or release is part of this phase. Published
main/v0.13.0 remain `b24361755e15c1a96d5b56d115499b9d4a9ef07f`. Full cross-platform
CI/version closing remains later; the local receipt records the observed branch
CI state when custody is released.
