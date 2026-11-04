# Safe automatic release of obstructed held props

Previously the player could leave an obstructed held body over ten metres
behind without releasing it. The ordinary carry controller now releases when
both conditions hold: sustained obstruction exceeds **0.35 s**, and the distance
from the player's eye to the rigid body's centre exceeds
**2.2 m pickup reach + 0.5 m margin + the shape's bounding radius**.
The size allowance is conservative for spheres, boxes, hulls and compounds.
Nearby blockage and ordinary progress toward the hand retain custody.

The policy runs after carrying/physics advancement. It uses the same checked
current-pose clearance as ordinary safe release. If another solid occupies that
pose, it waits and retries rather than forcing an overlapping drop. A successful
release retains the current transform, clears linear/angular motion and forces,
changes the existing body to dynamic, and clears held ownership. World time then
governs gravity; paused/zero-scale physics keeps the released body at that pose
until simulation resumes. No body is removed, duplicated, moved toward the
remote player or given the player's remote velocity.

The gameplay owner invalidates pending carry input/targets, clears stale release
feedback and immediately updates semantic held identity. The developer trace
records `item_auto_dropped` and its key in per-frame physics stats. The existing
obstruction advisory remains gated by Developer Console. No new normal HUD
diagnostic, setting, map field or checkpoint schema is introduced.

Checkpoints and elevator transport retain ordinary held/unheld serialization.
Restoring a held body starts with fresh obstruction grace and replans carrying;
the release policy tests the receiving coordinates. A saved auto-dropped body
restores unheld, and a valid cabin-relative held prop remains within range.

## Source validation

Integrated base: `aacc0e7c`, including the flush doorway, light containment and
current carry-advisory work. Strict workspace/all-target/all-feature Clippy passes.
All **450 workspace tests** pass, approximately 1.21 s of test runtime with
compilation separate. Three new CPU regressions use one sphere and one slab:
two object sizes immediately below/above their distance limit, unchanged release
and render pose with zero impulse, fresh checkpoint grace/released ownership,
ordinary following over ten metres and ordinary manual release. They load no
assets, use no native device and bake no map. Existing safe-release regressions
continue rejecting actual penetration and permitting ordinary resting contact.

The implementation does not modify elevator artwork, wall/editor placement,
door animation, passenger selection or destination configuration. Preparation
identity changes because this is shared engine code; final integrated map
exports must use the final compiler. Only tiny explicit passenger fixtures are
prepared for this task, preserving shipped maps and other workers' outputs.

## Native reproduction

`tools/bench/auto_drop.py` consumes the exported straight source written by
`elevator_cargo_fixture.py`. `--mode normal` checks actual following, held
save/resume and manual release. `--mode obstructed` establishes a stuck body
inside a closed cabin, then prescribes the player beyond the sealed door and
uses ordinary W input across the release distance. It preserves raw actions,
frame/body traces, checkpoints, captures and exact executable/package hashes.
The setup does not claim that the player naturally walked through a closed door.
Native sessions are serialized with other game/editor validators.

The integrated native crate's limit is **2.851554 m**. It stays held at
**2.731772 m**, then releases at **2.871075 m** as ordinary walking crosses the
boundary. The release event has zero added linear/angular velocity and preserves
the checked pose. Released save/resume keeps the body unheld. The independent
normal-carry run follows actual walking, retains held save/resume and permits
ordinary manual release. The existing advisory off/on/off check still passes.

Evidence lives under `local/auto-drop-integrated/`; the earlier component-only
campaign is preserved separately under `local/auto-drop/`. The game executable
is `local/auto-drop-integrated/production/places`, SHA-256
`33785dda3ab3fbbac01badf0062a5796ab0509e16a9bf3c60aabd2bf9bb74a79`.
Compiler SHA-256:
`f1a397c4956dc0fa446fc84d4a5e3c1ca8307b99fc6abf38a880520c1d0e560a`.
These are actual macOS Metal direct-binary correctness captures with ordinary
display pacing, not throughput or cross-platform claims.

All six integrated native cases pass: normal following, obstructed auto-release,
advisory visibility, straight held cargo, rotated held cargo and held-cargo
level-load failure. The cargo campaigns cover same-map travel, both cross-level
directions, local/foreign save resume, NPC state, outside-object exclusion and
single-instance return. Failed loading retains source poses and held ownership,
reopens the doors and leaves movement responsive. The fault-injection helper now
parses only complete JSONL records so a partially written trace tail cannot abort
that validation.

The locked full-workspace/all-feature release build, formatting, Python syntax
and diff whitespace checks also pass. Both explicit tiny fixture packages verify
current and integrity-valid for both member levels. Final shipped/demo exports
remain the integration owner's refresh against the final compiler; this task
does not replace other workers' distributions or perform a shipped-map bake.

The release deliberately waits if the current held pose fails clearance, and
freshly restored held objects receive the normal grace period. It applies to
ordinary player-held rigid bodies; authored NPC assemblies keep their separate
ownership controller. No release or merge to main is performed.
