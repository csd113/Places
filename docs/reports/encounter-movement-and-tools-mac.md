# Encounter movement and tool presentation — 0.14.0

The encounter now keeps its usable route while a moving target waits for the
next budgeted path search. Previously, target updates discarded that route,
leaving an actor stationary until another search was allowed. Explicit holds
still stop immediately, and all movement retains navigation/body collision checks.
A replan also skips its projected start-cell centre when the next segment is
independently clear from the actual actor position, avoiding a tiny backward
turn at every route replacement.

Named encounter gaits now use measured displacement and each clip's reference
speed. Previously, the robot's 1 m/s walk played at normal speed while its body
moved at 3.75 m/s. Zombies had the same mismatch with their 2.2446 m/s run.
Robots now have a crowbar run derived from the existing run, with their upright
grip retained. Zombies retain the existing arms-forward run and standing pose.

Encounter movement accelerates, slows for tight turns, and brakes into attack
reach. Stationary fighters turn toward their opponent at the existing bounded
turn rate. Robots retain an equally suitable opponent until another is clearly
nearer, while their own pursuers always take precedence. Short character-only
entry blends replace the abrupt locomotion-to-swing transition; damage still
uses the existing gameplay clock and contact fraction. Collision-owned exact
animation samples retain their existing instantaneous behavior.

The crowbar is an original closed Blender mesh with a longer forged hook,
bevelled edges and split heel. Its original GIMP enamel/steel atlas is a saved
PNG, embedded identically in the GLB. The existing mannequin arm geometry and
UVs are unchanged. The grip is slightly higher, and the hook turns inward.
[Half-Life 2 gameplay](https://steamcommunity.com/sharedfiles/filedetails/?id=186579520)
was a hold/framing reference; no Valve artwork or geometry is included.

The health hearts were redrawn in GIMP with a 34% centre and 88% edge before HUD
tinting. Their half-heart values, odd maxima, numeric fallback, optional display,
and map opt-in behavior are unchanged. Every unrelated pixel in both the game
and editor font atlases compares exactly with the previous files. Editable
Blender and GIMP masters live under `tools/player/source/`.

## Source and asset validation

- Strict all-workspace/all-target/all-feature Clippy passed.
- `cargo build --locked --release --workspace --all-features` passed on the final source/assets.
- `cargo test --workspace`: 591 passed, none failed or ignored.
- Two small CPU regressions cover moving-target route continuity/explicit holds
  and named-gait/action timing, without assets, a GPU or full-map baking.
- The 60 Hz mannequin skinning/contact sweep reports no problems. All 42 run
  samples touch the floor; minimum robot-run vertex height is −0.54 mm, and
  minimum arms-forward run height is −0.05 mm. These are authored flat-floor
  samples, not a claim of runtime foot IK on arbitrary terrain.
- The 19 original mannequin clips, rig, geometry, UVs and textures are retained.
  The new model contains 25 clips and uses ordinary lighting without cavity data.
- Catalog validation passed with 17 existing placement warnings outside this work.
- New PNG embedding, dimensions, transparency and atlas preservation checks passed.

## Native evidence

Evidence is retained outside `target/` in
`local/verification/movement-20261110/`. It includes the unchanged baseline
recording from commit `7ac38409`, isolated fixture, input recipes, source hashes,
asset exports, and validation logs. The fixture uses five robots centred at
`(16,17)` and the original two zones with ten zombies each. The shipped single
companion lab is unchanged. The developer overview camera is recording tooling,
not a spectator feature.

Recordings use actual native framebuffer pixels, fixed 1/60-second simulation
steps and every second frame encoded at 30 fps. They show simulation timing even
when readback slows wall-clock execution; they are not FPS benchmarks. Combat
recordings use ordinary attacks. Separate lifecycle diagnostics inject lethal
damage to test ownership cleanup and must not be presented as natural combat.

Windows remains deferred. Local crowd avoidance is still basic, and the work
does not add runtime foot IK, motion matching, multiplayer or waves.


The final Mac/Metal production runs completed two natural five-robot encounters,
with all twenty zombies dead and all five robots alive by roughly 18 encoded
seconds in each. The delivered `movement-overview.mp4` and `movement-close.mp4`
are the continuous opening 22 seconds, trimming idle aftermath without changing
playback speed or re-encoding. The full 36.50/36.57-second captures remain beside
their recipes and traces. Frame counts match the native capture events, and both
delivered files decode without errors. The close camera is `(7,1.8,18)`; the
matching before/after overview is `(16,2,25)`. Sequential frames were inspected
for stride, contact, turning, stopping and attack transitions.

First-person checks cover idle, swing, look-down, holstering, half health and
health disabled at 45°, 60° and 110° FOV. The final production binary repeated
the 60° presentation check after the atlas packing correction. The body stays
hidden, the tool appears only when equipped, and disabled health adds no HUD.

A separate final-production lifecycle run exercises three twenty-zombie rounds,
three E respawns and 32 rapid group toggles in one process. Each round creates
all 26 ragdolls through diagnostic lethal damage. Entity/component/AI/render and
solver ownership return to the warmed baseline after every respawn and after
the toggles. Baseline is one body, seven colliders, no joints, no ragdolls and no
bone-pose caches. Cleared RSS samples are 351.14, 352.91 and 321.17 MiB: no
sustained rise in this short campaign, not a proof against every leak.

Final player binary SHA-256:
`bd294fa47703010518eeef85dc70964f11bc11f0aa93db09f9108979cc34da98`.
The player, editor and compiler are also copied and hash-verified under
`preserved-build/bin/` in the evidence directory.

The requested `cargo clean` completed after confirming no other active Places
task or build/game/capture process was using the directory. All 277 evidence
files were hash-verified before and after cleanup, including the three preserved
production executables. The source changes remain uncommitted for review.
