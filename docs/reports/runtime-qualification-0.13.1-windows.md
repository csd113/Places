# Runtime interaction qualification - Windows, 30 October 2026

The current runtime passes the exercised Home, Pool, dormant-contact and authored
Relay recovery cases. No new gameplay fault or quality-preserving FPS optimization
was established. The demonstrated defect was in the benchmark: the old Home books
spawn overlaps the bedroom wall/bed clearance, is pushed outside the map and can
render nonzero geometry while falling indefinitely. The timing harness previously
accepted that run. The corrected side approach and crouch-jump input now exercise
the actual bed, and `frame_time.py` rejects the engine's no-clear-recovery-spawn
warning before publishing a receipt. No runtime behavior or artwork is changed.
The rejected recipe reaches eye Y=-134.694 m; the corrected control lands with
feet Y=0.55 m / standing eye Y=2.15 m and returns to floor Y=0 with zero recovery.

## Cases and limits

| Requested case | Current result |
| --- | --- |
| Home idle, pickup, carry, release and settling | EXISTING-PASS: cushion and bedroom books retain ownership/key, release and sleep. Fourteen authored rigid items remain present. |
| Bed, jump, landing and recovery | EXISTING-PASS from a clear side spawn: standing jump refuses the 0.55 m bed; crouch jump lands on it, standing expansion succeeds, retreat returns to the floor with zero recovery. The old wall-overlap recipe is excluded. |
| Object-count scaling | EXISTING-PASS for bounded CPU controls: 12/32/64/128 bodies, contact/water/dormant scenarios, exact one/four-worker state parity. Native 25-body Lab contact wakes 18 bodies, then separation permits sleep; its cushion pickup/release also passes. This is not a 128-body rendered Home FPS claim. |
| Pool approach, door, enter/push, face away and retreat | EXISTING-PASS: real doorway crossing wakes/pushes two supplies and retreats through the opening. The door's real swing pushes the initial approach sideways; the accepted control explicitly resets to a clear exterior point after opening, then walks through. |
| Frozen top/jump contacts, fixed and held behavior | Existing CPU top/side/sleeping regressions pass. Native side/separation and held ownership pass; Home validates fixed-bed jump/standing clearance. Final-production top/jump/side smoke adds 14 checked conditions. |
| Original second Annex section | EXISTING-PASS: jump ascent, downward diagonal and crouch at 6.94/33.33/100 ms simulation intervals; grounded completion and zero recovery. Side/entry attempts refuse authored rails/separators without falling. **Direct exposed-riser-face native contact remains UNVERIFIED** because those barriers shield the attempted approaches. No substitute map or speculative collision rewrite is claimed. |
| Authored essential recovery | EXISTING-PASS: original Relay Intake supply returns from its well and void with the same key. Two blocked automatic/manual resets preserve complete before/after body state and held ownership; clearing the destination makes ordinary sign use succeed. Unrelated supply remains unchanged. Delivery rings its authored bell exactly once; repeated sign use preserves accepted progress and the delivered pose. |
| Transition and save | EXISTING-PASS for the authored contract: airlock enters fresh Tower state with empty hands. Intake disables saving; a real checkpoint request refuses and writes no gameplay save. No checkpoint or cross-level inventory guarantee is invented. |

The 47 initial focused native assertions pass, with final-binary smoke recorded
separately. Raw failed/setup attempts are retained and excluded: the invalid Home
spawn, Pool routes that did not cross the opening, invalid SDL/action names, a
resolution-runner import failure after native completion, and side approaches
that tested the rail rather than an exposed riser. Run selection is by valid
setup/identity, never the fastest replay. Full-frame PNGs are unchanged native
captures; no visual asset was authored or replaced.
The rebuilt player's falling/jump top-contact controls wake the original cushion
while the other 24 Lab bodies remain dormant. Full standing clearance is refused
while that prop occupies the hull; walking away restores standing with zero
recovery. The maintained Home carry recipe also passes unchanged, and a real
mid-flight Annex side contact/separation stays supported against the authored rail.

## Production cost, pacing and memory

Windows / RTX 3070 Laptop / DX12 and DXC. Three runs per view, reversed order in
round 2, 1,800 warmup plus 1,200 measured frames each: **21,600 measured frames**.
Actual Immediate, swap interval zero, V-sync/limiters off, physical 1920x1080,
FOV 60, High textures/filtering, Full lightmaps/reflections, bloom on. Every
accepted measured interval has 100% sampled foreground ownership, no missed
ready presentation, exact package/member identity and real scene submissions.
Game/perf runs are serial at Normal priority; builds/bakes are BelowNormal.

Values are medians of run statistics. FPS is 1000/mean loop time within each run;
it is not the reciprocal of the displayed median frame time. GPU sample IDs are
deduplicated. CPU/GPU spans overlap and must not be added. RSS ranges are sampled
whole-process peaks including loading, excluding driver/GPU allocations.

| View | Mean FPS | Loop median / p95 / p99 ms | CPU update / render median ms | GPU median ms | Peak RSS MiB, range |
| --- | ---: | ---: | ---: | ---: | ---: |
| Home, cushion idle | 233.4 | 4.470 / 6.813 / 7.519 | 0.208 / 0.584 | 3.198 | 509.9-510.6 |
| Home, cushion held | 223.7 | 4.935 / 6.961 / 7.462 | 0.444 / 0.581 | 3.284 | 512.3-512.9 |
| Home, two books settled on bed | 163.7 | 6.466 / 7.844 / 8.241 | 0.353 / 0.780 | 6.371 | 517.3-547.0 |
| Pool, far approach | 189.7 | 7.018 / 9.951 / 10.515 | 0.346 / 0.683 | 3.632 | 583.3-599.0 |
| Pool, closet inside | 230.7 | 4.651 / 6.793 / 7.431 | 0.288 / 0.593 | 3.325 | 582.7-608.5 |
| Pool, facing away | 150.8 | 7.234 / 8.901 / 9.857 | 0.318 / 0.940 | 5.894 | 622.2-639.8 |

The 165 Hz display does not establish a cap for the bed view: the matched
1280x720 control reaches **348.5 FPS / 2.646 ms GPU**, versus 163.5-170.0 FPS /
about 6.371 ms GPU at 1920x1080. This lower-resolution diagnostic is evidence of
workload response, not a shipping quality change or implemented gain. Pool's
third face-away run is slower (132.3 FPS); it is retained in the raw range rather
than discarded. No before/after optimization or consistent tail improvement is
claimed by these current-state measurements.

Separate logging-enabled attribution finds sleeping bed-book physics about
0.102 ms median, zero awake bodies, dynamic lighting 0.0116 ms, receiver update
0.0036 ms and character lighting 0.0007 ms. Three sparse renderer samples acquire
in 0.0085-0.0131 ms. CPU update/render are below 1 ms while this view's GPU work is
about 6 ms. Presentation/driver waits remain visible in swap time; they are not
labelled physics or acquire cost. Moving Pool supplies produce about 0.904 ms
receiver work and 1.000 ms dynamic lighting in the diagnostic window; its whole
route's physics p95 is 0.112 ms. The historical global-water invalidation fix,
old/new caster bounds and receiver-envelope checks are unchanged.

The headless single-repeat scaling control has serial medians 0.085 to 0.457 ms
for 12 to 128 contact bodies, 0.079 to 0.604 ms with water, and 0.0009 to 0.0098 ms
dormant. It changes no solver-pool policy and is not a rendered FPS gain. No
additional game buffers, textures or runtime storage are introduced by this
benchmark-only correction.

## Identity, currency and gates

Baseline code: `db333eeca2205803dbf60a759c9e1455a3d114b1`; all 575 Rust source
hashes remain unchanged. Native production player SHA-256:
`ac9575cc32d1edd29e5d1e8640acf461f8489a265f026bb1d303cd7c28e50461`.
Preserved compiler: `a2a794ef4aee7d132b6a867bf5e783c3da63d7b7a8909f59c1cf4639ea97738b`.
The original environments archive remains
`749c9f3e45120d7c72416891dd5d4f5965739dc88b60b5827f06fb1e33ac28f5`.
Final rebuilt executables/DLLs and hashes are preserved outside Cargo target.
Rebuilt player SHA-256: `98674d09b02991532cd023d2f176f36c5f95d8fb13cbcd00beead73364a81bbe`;
rebuilt compiler: `035f9be2706ce75cbdfcf66488c41fdaa00fff1bf97ea0863d228e2842ac8c77`.
The six runtime DLLs match the preserved reference. The main timing campaign
uses the preserved player with an exact 575-file built-source-manifest match;
fresh native contact/clearance smoke uses the rebuilt player.

Only targeted Relay Annex, Performance Lab and a tiny contact fixture were
prepared; the latter is not native acceptance evidence. Relay's three members
and Lab validate and verify current. Home/Pool selected verification reports
`integrity_valid=true`, with only conservative compiler-source currency stale.
Whole-collection current-catalog validation refuses Winter's referenced catalog
closure after the previously accepted rail update. The complete old archive
validates with its preserved matching assets. No stale check was suppressed,
identity relabelled, compatibility layer added or shipped collection rebaked.

| Gate | Result |
| --- | --- |
| fmt | PASS |
| clippy | PASS |
| test | PASS |
| release | PASS |
| clippy-release | PASS |
| python | PASS |
| assets | PASS |

The exact commands and exits are in `gates/*-result.json`: required strict
workspace/all-target/all-feature Clippy, locked release workspace/all-feature
build, release Clippy, 399 Rust tests, 372 Python tests (18 expected skips),
format and 674 catalog assets with zero warnings. The single new Python
regression protects the actual invalid-workload qualification bug using one
warning string, with no executable, GPU, asset decode, bake or timing assertion;
the 16-test benchmark-accounting group takes about 0.01 seconds.

Evidence root:
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-30/task/runtime-evidence/`.
`native-assertions.json`, `fresh-native-assertions.json`, `performance-summary.json`,
`diagnostic-summary.json`, `physics-scaling-summary.json`, per-run identity/
process/foreground/RSS receipts, raw CSV/log/PNG files, package verification and
source hashes preserve reproduction. The unrelated dirty menu scene remains
unstaged and byte-identical, SHA-256
`93b92d7d6caaa9a7fc1a1bec2bb0805c17cccd25f584c730ef882ffbbcf9a27a`.

## Local queue and publication

`local-implementation-queue.json` explicitly records the implemented Home
qualification repair, remaining original-map exposed-riser native coverage, and
affected Winter/collection currency refresh at the existing final packaging step.
The prior `TR-PLANAR-CHARACTER` proposal remains separate and unchanged. Hollow
character-lighting, later startup ownership and larger architecture work remain
in their existing measured queue; none is silently absorbed here.

Publication remains deferred. The prior automatic approval denial of pushing
renderer commit `db333ee` also blocks publishing descendants; no push or alternate
publication route was attempted. The earlier #24 body edit and reflection-issue
creation denials were not retried. Their exact payloads/reasons remain in the
task-8 handoff and this task's publication handoff. No new CI, release, main merge,
Mac validation, GUI Bake or portable-package publication is claimed.
