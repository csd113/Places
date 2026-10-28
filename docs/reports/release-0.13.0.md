# Places 0.13.0 implementation and validation report

This is the consolidated report for the approved 0.13.0 queue on
`0.13.0-staging`. Sections retain the evidence and limitations recorded at each
stage; earlier next-stage statements are historical handoffs. Raw local receipts
and direct binaries remain outside `target` in their original stage directories.
No main merge or release is implied by this staging report.

The [latest RAM handoff](#runtime-ram-phase-4---windows-2026-10-24) records
substantial Lantern memory savings and the approved shared-geometry/automatic
instancing stage before remaining runtime/editor work. The editor bake-progress
bar and earlier accepted gains are retained. Earlier next-stage statements are
historical cohort records. The [final combined Windows checkpoint](#final-combined-windows-validation-and-portable-package-2026-10-24)
records the final portable-player checks and remaining performance limits.
The [Mac/Metal acceptance](#apple-silicon-mac-acceptance-2026-10-27) records the
final uploaded combination, the non-Windows lint repair, current native packages
and the refresh-pacing limitation. Linux coverage remains open.


## Configurable Sun and Moon - native Windows, 2026-10-22

Outdoor scenes can opt into fitted Sun/Moon PNGs on their existing directional
sources. The visible position and the baked incoming light use **one shared
negative normalized direction**, so image placement cannot drift away from
illumination or shadows. The existing editor form exposes bearing/elevation,
show texture, visual diameter and brightness alongside source energy/color,
shadow softness and enable/bake controls. Old scenes omit `visual` and retain
their authored behavior; imports do not add a sun or moon automatically.

The two real transparent 512-square PNGs are
`assets/environments/outdoor/textures/celestial/{sun,moon}.png`, produced with the
built-in ImageGen tool and uniformly resized with alpha preserved. Exact prompts
and provenance are under `assets/references/environments/celestial/`. The new
asset contract is documented in `ASSET_SPECIFICATION.md`. No shader/runtime
texture painting, copied lighting system or dynamic day/night simulation was added.

Evidence below is relative to `celestial-additions/` in the Windows validation
root. `native-celestial-completed-audit.json` passes all five final native cases:

| Check | Actual result |
| --- | --- |
| West/east Sun, Moon | Full native lighting, visible textured discs, correct reversed pillar shadows, ordinary panorama and no-panorama clear-background case pass. |
| Occlusion | Opaque canopy fully covers both celestial discs; source remains visible in the open view. Preliminary edge-framing samples remain preserved separately. |
| Real editor placement | Visible form changes Sun to 315-degree bearing / 50-degree elevation, 12-degree visual diameter / 0.85 brightness. Preview leaves source untouched; Apply/save/reopen retains the settings. |
| Build / Play / export / recovery | Ordinary editor Build and real Play child pass. Export/import recovers all four project files byte-for-byte. A final-compiler export of that exact recovery renders the larger disc and corresponding diagonal shadows natively. |
| Visual-only cache reuse | Diameter/brightness edits refresh package semantics while reusing every solved geometry/lightmap/probe/collision record. Replacing background-only image bytes is detected by package identity and also reuses solved records. A native launch of the actual reused-lighting package shows the larger/dimmer disc with unchanged source shadows. |
| Real bake invalidation | Moving the directional source makes the old package stale and produces different compiled lighting. If the celestial PNG is also sampled by a world material, changing it invalidates and rebuilds lighting. Production imagery is untouched by these isolated substitution tests. |
| Portable payload | Player renders Sun then returns to the normal menu; real editor preview and compiler package validation pass outside the checkout with asset-root overrides unset and **only Windows system directories in PATH**. Sibling assets/SDL3/DXC are sufficient. |

Every final capture confirms the actual **NVIDIA RTX 3070 Laptop / DX12** adapter,
1280x720, High/Full settings, VSync OFF and applied Immediate presentation. These
capture runs are excluded from FPS measurements. No new celestial throughput
percentage is claimed; the uncapped dense-scene evidence and its pacing limits
remain below.

A final cache review found and corrected unnecessary visual-only rebakes before
completion. Celestial appearance is excluded from the existing prepared-lighting
stage using its actual remaining material/model dependencies and catalog closure.
Real light properties remain in that key. The existing source-contract regression
also protects visual/light sign, old defaults, serialization and this separation.
The explicit native cache fixture is small and is never baked by Rust tests or CI.

Strict workspace/all-targets/all-features Clippy passes. Rust tests: **272 passed,
zero failed or ignored**, 1.01 seconds test execution. The final locked release
workspace/all-features build passes; logs are in `quality/`. Asset validation:
627 assets, 389 placeable, nine themes, zero warnings. Texture validation passes
with the same 96 existing advisories; source formatting/diff checks pass.

The existing sky pass caches both image bindings and source projections once per
level, bounds the source list at eight, executes no celestial loop for old skies
without visuals, and skips identical still-camera uniform uploads. No per-frame
lighting solve or automatic rebake is introduced. Backgrounds retain the existing
sky/reflection-capture contract. The small explicit review project is
`projects/celestial-placement`; no full showcase rebake was performed.

Current small exports are `packages/celestial-placement-cache-corrected.placesmap`,
`packages/celestial-editor-recovered-cache-corrected.placesmap` and
`packages/mannequin-appearances-cache-corrected.placesmap`; their SHA-256 values
are in the completed audit. Final eight executable hashes and earlier candidates
remain outside `target`. The usable player/editor/compiler payload is
`Places-Windows-0.13.0-additions/`, with a local
`Places-0.13.0-Windows-additions.zip` bundle containing the current nine-map
collection, small fixtures, editable sources, selected native captures and the
honestly labeled miniature before/after montage. Earlier baseline folders,
archives, settings and measurement receipts remain intact for the next sole writer.

The Balanced power plan is unchanged. Temporary **system-required only** helper
PID 18136 remains active for the approved following queue, on AC at 100% battery
at handoff. Its owner must clear the request with the recorded stop file after
queued work or cancellation; the 24-hour safety expiry and low-battery guard
remain in force. Build outputs are retained for Portal 1 movement/controller work and later
stages. No CI wait, Library executable upload, release, version bump or main merge
occurred. Native Linux remains unverified.

## Cosmic Lanes miniatures and shared appearances - native Windows, 2026-10-22

The subtle switch beside lane one's ball return now changes its ten pins into
quarter-scale mannequins. Real bowling-ball contact replaces each standing body
with the existing 15-body/14-joint ragdoll, preserving impact velocity once.
The normal reset restores the original pins, and the blue-ball ten-pin favor
still completes and saves through ordinary interactions. The secret mode has
its own NPC guidance and cannot accidentally complete that favor.

Man, Woman and non-graphic Zombie appearances use three fitted opaque 1024-square
PNG sheets with the unchanged canonical mannequin GLB (37 joints, 19 clips).
Original and Black remain available. The editor's Gameplay > Appearance choice
works for placed actors and spawn templates. Immutable mesh, rig and compiled
clip data are shared; each actor retains independent animation state and a
cached appearance binding. No model/animation copies or GPU skinning were added.
Texture references participate in validation, dependency hashes and catalog
closure; required package capabilities reject unsupported readers by name.

Native evidence is under `mannequin-additions/` in the Windows validation root
above. `native-feature-audit.json` passes the actual functional assertions:

| Check | Native result |
| --- | --- |
| Moving appearances | Man walks while Woman runs and Zombie holds a separate pose; all three fitted textures and Original/Black visually inspected on the model. Seven actors share two mesh resources (the mannequin and the switch model). |
| Skin physics | Real interactions create three independent ragdolls (45 bones/42 joints); reset removes them. Textured spawned guest animates independently. |
| Editor round trip | Visible five-choice picker, save/reopen, Build, real Play child, export/import and editable recovery pass. Recovered JSON files are byte-identical. Recovered Zombie and runtime Black templates both visibly correct. |
| Miniature impacts | Ball contacts produce up to ten real rigs: 150 bones/140 joints. Standing proxies are removed before bone insertion; aliases retire on reset/unload. Mass scales with volume. |
| Settling repair | Quarter-scale rigs exposed persistent contact chatter. Scale-aware angular damping/contact padding fix it without relaxing sleep thresholds or forcing sleep. The old deterministic ten-rig regression fails; the corrected one passes in under one second. Native after capture reaches **150/150 naturally sleeping**, versus 105/120 in the distinct earlier eight-strike capture. |
| Repeated reset/toggle | Normal reset and three toggle cycles restore exactly 106 bodies/374 colliders/zero joints or ragdolls and 24 original characters; restored rack visually inspected. |
| Ordinary quest | Ten original pins fall from a real blue-ball throw; ordinary NPC completion writes exactly `arcade` into a fresh save and repeat dialogue is idempotent. Existing all-eight campaign acceptance remains recorded below. |
| Unload | Ten live rigs then return to menu restores 19 bodies/202 colliders/zero joints or ragdolls, with no retired Arcade prepared owners retained. Menu vignette actors vary; total process RSS/VRAM return is not claimed. |

Locked release workspace/all-features build, strict all-targets/all-features
Clippy and formatting/diff checks pass. Rust tests: **271 passed, zero failed or
ignored**. Asset gate: 625 assets, 389 placeable, nine themes, zero warnings.
Texture and prop gates pass with existing advisories. Final cached production
build was reconfirmed before preserving all eight executables outside `target`.

All timing uses the actual NVIDIA RTX 3070 Laptop / DX12 renderer, VSync OFF,
applied **Immediate**, 1920x1080, High textures/filtering, Static Arcade lighting
(runtime Low, lightmaps/reflections off), no fixed delta/limiter/captures and no
competing compiler. The ready interval is seconds 40-55, using actual loop time:

| Run | Actual struck miniatures | Mean FPS | Mean loop / update | Loop p95 |
| --- | --- | --- | --- | --- |
| Before settling correction | 10 | 161.68 | 6.185 / 1.825 ms | 6.914 ms |
| After, first physical roll | 9 | 162.60 | 6.150 / 1.807 ms | 6.772 ms |
| After, repeated physical roll | 9 | 167.57 | 5.967 / 1.692 ms | 9.181 ms |

These are **not matched throughput gains**: physical strike counts differ and
presentation pacing remains material (the first two runs cluster near 165 Hz;
the repeat has more short frames and a wider tail). No percentage saving or
cross-Mac speedup is claimed. The ten-rig native settling capture is separate
from timing. Raw CSV/identity/receipts preserve the imperfect rolls rather than
silently selecting a favorable comparison.

The requested local montage is
`mannequin-additions/media/miniature-settling-before-after-sampled.avi`
(11.4 seconds, MJPEG AVI). It compares genuine before/after screenshot samples;
the 31-second wait is omitted, nominal sampling is 280 ms and playback samples
are 300 ms. Contact-count differences and timing exclusion are visible on the
video. SHA-256: `f3ce3ce2de78c841b9f792d9d9b822d5331acd325c8802cce7e378437b365d56`.
It is a settling comparison, not a fabricated original-release FPS baseline.

Current packages: `packages/environments-miniatures-native-mixed-2.placesmap`
(nine maps, SHA-256 `5172d05c3152bf40842bf68f15ed0f4aba52f9902d7586ae68ee142e4b0e6f2a`)
and `packages/mannequin-appearances-current.placesmap`. Only Arcade was rebuilt
as Static; eight existing members were preserved. Pool/Hub retain their Full
payloads; full-showcase lighting was not rebaked. Production assets and exact
built-in ImageGen prompts are under `assets/entities/mannequin/textures/` and
`assets/references/entities/mannequin-skins/`. Useful native tooling, screenshots
and all candidate executables are retained outside `target`. No Library upload,
CI wait, release, version bump or main merge occurred. Build outputs remain for
the approved sun/moon and subsequent queue work.

## Native Windows Razer validation - 2026-10-22

This is focused native Windows acceptance, not a full replay of the macOS
campaign or an authorization to release. The clean Windows checkout was safely
advanced from `main` to the authorized `0.13.0-staging` remote head
`c4a59a3af9453d09d65983a33193bd7779487a8a`, following
`536ee0faf7ed5f54524b1a1ce4f58db866dea85b`. No unrelated changes were present.
Origin is `https://github.com/csd113/Places.git`; remote staging was checked
again before preparing this change. No CI was queried or awaited.

All Windows paths below are relative to
`C:\Users\Connor Dawkins\Documents\Places\local\windows-validation-0.13.0`.
The one canonical report remains this file; JSON/logs are raw local receipts.

### Host and actual rendering device

- Razer Blade 14 RZ09-0370, Ryzen 9 5900HX, 8 cores/16 threads, 16 GB RAM;
  Windows 10 Home 19045; Rust/Cargo 1.99.1; SDL3 3.4.18.
- AMD Radeon integrated graphics driver `31.0.21924.61` drives the recorded
  2560x1440, 165 Hz display. NVIDIA RTX 3070 Laptop GPU has 8192 MiB VRAM,
  driver 617.14. Installed hardware and display attachment do not establish
  the rendering adapter.
- The original app selected **AMD / IntegratedGpu / DX12**. Its first launch
  hit the FXC shader compilation failure. An AMD DXC preflight rendered the
  native menu. This is not an AMD throughput benchmark.
- Candidate application requests HighPerformance from wgpu on Windows;
  actual `[renderer]` logs confirm **NVIDIA / DiscreteGpu / DX12** for every
  timing below. No system graphics settings, power profiles, security settings,
  driver installs or OS dependencies were changed.

### Repairs and quality gates

Windows release compilation exposed unsupported-platform dead code and Clippy
failures. Platform-specific GPU/RSS samplers now compile only where supported;
Windows retains explicit N/A counters instead of invented measurements. Native
and offline Windows renderers use matching sibling `dxcompiler.dll` and
`dxil.dll`, with an actionable missing-payload error rather than FXC fallback.
The existing SDK pair 1.8.2502.11 was used; Windows dependencies are documented
in `docs/DEVELOPMENT.md`. The application adapter preference also covers offline
rendering, without changing other platforms' default preference.

Real editor export exposed a Windows filesystem difference: its existing spool
**directory** at `<output>.partial` made exclusive file creation return
AccessDenied, whereas Unix returned AlreadyExists. Temporary creation now skips
occupied entries without changing permissions or discarding them; exclusive
creation still protects races. One tiny regression preserves both an occupied
spool directory and interrupted-file sentinel.

The raised-arms wall-contact ragdoll regression reproducibly failed to sleep
on Windows. Increasing joint damping from 2 to 4 fixes that feedback without
relaxing sleep thresholds or disabling contacts. All six existing ragdoll
regressions and native collapse/reset acceptance passed.

Benchmark helpers now tolerate missing Unix resource/load/RSS APIs and copy
isolated packages on Windows without requiring symlink privileges. Item tests
now set VSync **off**. Uncapped throw direction assertions read the real
`item_thrown` launch snapshots: delayed wall-time screenshots can already show
later collisions under fixed-step simulation. Continuous contact, floor,
identity and angular-motion assertions remain in force. The original failing
receipt and its re-audit are retained; a fresh corrected native run passed.

| Gate | Actual Windows result |
| --- | --- |
| Locked release workspace/all-features build | Pass; final rebuild 6m50s, `release-build-export-fixed.log` |
| Strict workspace/all-targets/all-features Clippy, `-D warnings` | Pass; `clippy-export-fixed.log` |
| Rust tests | **270 passed, 0 failed, 0 ignored**; `rust-tests-export-fixed.log` |
| Focused Python tests | 15 benchmark-metric + 7 environment validation tests passed |
| Assets | 622 assets, 389 placeable, 9 themes; zero warnings |
| Source formatting / diff / changed Python compilation | Pass |
| Entire Python suite, Windows installer, Linux native | Not run |

### Native acceptance and package custody

| Check | Result and practical limits |
| --- | --- |
| Menu and left-edge backdrop | Native 1280x720, 1200x900 and fullscreen 2560x1440 screenshots visually inspected; title/navigation and full-height left coverage pass. Capture runs are excluded from timing. |
| Explore | Current project entry, actual authored Hub thumbnail and description render correctly; `explore-native-state/view.png`. |
| Real editor pickers | Existing prop picker selects `core:chair`, query/theme/no-match cases and cold/warm thumbnails; floor material picker visually inspected. Scratch project only. |
| Editor persistence / compiler / Play | Save/reopen retains six props; native Build, real Build+Play child and return to editing pass. Final project export is 3,010,409 bytes. Native editable recovery preserves all three declared JSON/Luau files byte-for-byte. `editor-export-fixed-evidence/`, `editor-recovery-byte-check.json`. |
| Portable player/editor/compiler | Pass from an unrelated working directory with `PLACES_ASSET_ROOT` unset; native menu, editor reopen and compiler package validation use sibling assets. `portable-*` receipts. |
| Import/path handling | Unicode file/state paths, first/repeated import, duplicate-ID rejection, corrupt rejection and restart discovery outside checkout pass. Settings, save sentinel and installed bytes remain unchanged; no partial import remains. `import-path-acceptance/acceptance.json`. |
| Missing DXC | Native startup exits 1 with a precise sibling-DLL payload error and no panic; `missing-dxc-native/`. |
| Real pickup / mouse throw | Existing tiny Full fixture: **168 pickup checks, 293 throw checks passed**. Four launch snapshots preserve the original body; launch direction, angular response and floor/contact checks pass. `item-pickup/`, `item-throw-fixed/`. |
| Pool prop throw / water | Native Full Pool pickup, closet-door interaction, held movement and left-click release preserve original box key `9223372036854775862`. Full water/lighting captures visually inspected. This is a diagnostic-position setup, not a complete walking-route claim. |
| Animated NPC / ragdoll | Animation, fall/contact, 15/15 sleeping bodies, reset, repeated fall/sleep/reset pass with empty script diagnostics; `ragdoll-demo-state/`. |
| Furniture movement / crouch / stairs | Static-only current source: real held movement crosses the low closet crouched at 0.8 m eye height; climbs 18 stairs to 3.5 m feet height and descends to 0 m. Zero recovery resets. Diagnostic starting positions are explicit; `furniture-traversal-state/`. |
| Pool favor / saved progress | Follow-up ordinary walk from the authored spawn now passes bench/door/deck traversal, real talk, duck pickup, release and four-second upright buoyancy objective. Native success image and actual campaign save retained. Restart remembers the favor; real return portal loads Hub and ledger reports **1/8**. No save flags were injected. `pool-route-reviewed-state/quest-complete.png`, `pool-route-reviewed-state/saves/`, `pool-saved-transition/`. |
| Reward gating / room | Locked at 0/8 and 1/8; genuine 8/8 save unlocks and opens the door. Ordinary Arcade return and Hub walk enter the actual room, converse with its host and interact with all eight keepsakes. Full Hub restart and byte-identical repeat saves pass. `eight-quest-native-audit.json`, `campaign-eight-return-3/`, `campaign-eight-restart-2/`. |
| Retained work | Menu -> Full Pool -> menu returns prepared bytes to 105,567,345 and solver resources to 19 bodies / 202 colliders / 0 joints / 0 ragdolls. All retired build/record/transport owners are zero. Dynamic menu actors differ by event; RSS is unavailable on Windows, so this does not prove driver/total-RAM reclamation. `native-acceptance-audit.json`, `retention-native/`. |

No exported maps existed in the initial Windows checkout. Native compiler builds
produced Full tiny item/ragdoll fixtures, Pool and Hub. Standalone Pool correctly
refuses installation because its transitions require the intact project. The
nine-map project was therefore exported with its fast Static variant, then an
explicit diagnostic mixed project retained those sources and replaced **only**
Pool/Hub record maps with byte-identical native compiler Static+Full records.
All records remain content addressed. Official compiler validation and
`verify --require-current` for Pool and furniture passed.
`mixed-project-provenance.json` records all original and resulting hashes.

The current source project includes the eight quests and furnished reward room.
`projects/untitled-1792640069720/places.project.json` also exists here, with ten
main spaces across two storeys, 204 props and 37 fixtures. The Mac polished
package was not present. A full furniture bake was deliberately stopped after
199.5 s to keep validation practical; its owned-process identity, partial/cache
files and cancellation receipt were preserved. Its 2,335,145-byte Static export
is accepted for mechanics only, not polished Full lighting. No full showcase
rebake was spent on this Windows pass.

The original Pool walking recipe clipped `pool_imported_17`, the fixed perimeter
bench at (2.0, 9.7), and stopped forward at z=9.21. It then tried turning east
before clearing the partition at z=10.85..11.15. The open doorway spans
x=2.18..3.62; player capsule clearance stopped the eastward move at x=3.32,
z=10.9329. The door was already fully open (-90 degrees). This was an invalid
open-loop test route, not a demonstrated gameplay collision defect.

**Pool route follow-up passed:** the unchanged preserved executable now walks
from the actual authored spawn using only ordinary controls: pass the bench at
x=3.5, approach the opening at (2.9, 10.35), open the door, clear the partition to
z=12.6, cross the dry deck, pick up the real duck, carry and release it into the
water, and complete the four-second upright objective. All 51 actions execute,
all captures have zero player recovery resets and empty script diagnostics,
and the fresh native campaign save contains exactly `completed:["pool"]`.
There is no player/item pose injection, map change, engine change, rebake or
rebuild. `pool-route-review.json`, `pool-route-reviewed/` and
`pool-route-reviewed-state/` retain the input, positions, screenshots and save.
The runtime SHA remains the one recorded below; `target` remains absent.

The initial diagnostic-position objective remains historical evidence. Earlier
actor-only camera commands for authored props were correctly rejected and then
replaced with camera-only `view_at` inputs; those input errors remain preserved.

### Completed eight-favor native Windows campaign

The initial representative-only checkpoint is superseded: all eight favors now
complete natively on Windows with the preserved final player and the same
installed nine-map package/save identity. No completion flags are generated,
inserted or transplanted. Real save bytes progress from Pool to the other seven
favors, followed by an authored Arcade return to Hub and actual reward-room
entry. `eight-quest-native-audit.json` checks each native state and save change.

| Favor | Native completion evidence |
| --- | --- |
| Pool | `pool-route-reviewed`: ordinary authored-spawn route, talk, pickup, carry, release and upright buoyancy; original real Pool save seeds this same campaign. |
| Aero | `campaign-aero-1`: ordinary spawn walk, blue + lime pressure five, gardener reward and repeated completed dialogue. |
| Home | `campaign-home-2`: ordinary spawn pickup and carried route, actual NPC handoff, then physically leave its sight into the entry hall. The whole actor/cushion assembly is hidden before retirement and completion; only the expected authored informational notice is present. |
| Lantern Hollow | `campaign-jun-1`: normal talk, real collision-aware return navigation, `quest_seated` animation, completion and repeat seated interaction. |
| Beach | `campaign-beach-4`: normal keeper/crab talk and real escort with one-to-six-metre companion clearance; actual crab reaches the parasol and completes. |
| Winter | `campaign-winter-2`: ordinary spawn approach, continuous slippery movement across all three marks, three-lantern acknowledgement, coach return and repeat dialogue. |
| Office | `campaign-office-1`: three real pickups and collision-constrained carried returns, flat releases into the real open crate. All three pages sleep within the objective volume, at Y approximately 7.2592 / 7.2372 / 7.2624 m. |
| Arcade | `campaign-arcade-2`: normal attendant task, required blue ball pickup and mouse throw from Z=20.4, ten physically fallen pins, saved completion and repeated completed dialogue. The earlier nine-pin attempt remains incomplete evidence. |

Pool uses Full lighting. The other seven favors use their supported Static
variant (runtime Low lighting, lightmaps/reflections off); this is gameplay
acceptance and does not imply their Full lighting acceptance. Home, Aero,
Winter, Pool and Arcade use ordinary authored-spawn movement. Jun and Beach
use explicitly labeled player-only initial setup near the task; Office uses
player-only top-floor/station setup while empty-handed. Every pickup, carried
return, throw, drop, NPC handoff, NPC walk/seating and objective is real. There
are no item/NPC pose or velocity injections, script activation shortcuts,
console completion commands or invented progress. Complete outer-map approach
walks for those diagnostic setups are not claimed.

`campaign-eight-return-3` follows the normal Arcade return portal with the
actual eight-entry save, sees the Hub welcome and door unlock/open to 90 degrees,
walks around the real open leaf, enters at approximately (30.99, 1.60, 7.00),
speaks to the host and receives all eight distinct keepsake messages. This
transition uses Static lighting. `campaign-eight-restart-2` is a separate native
**Full-quality Hub** restart: automatic welcome, ordinary room entry and repeated
host dialogue pass again. `campaign-reward-final-visible/` retains visually
reviewed Full native images of both furnished keepsake tables.

Fresh native revisits to Home, Pool, Office, Winter, Aero, Beach and Jun return
completed dialogue twice, preserving byte-identical eight-entry saves. Jun
restores the seated animation; repeated Aero talk leaves the reward dynamic-object
count unchanged. Together with the normal Arcade repeat and Full Hub restart,
this verifies persistence, restoration and idempotence. The same campaign key
is retained across every session and transition. All accepted captures have
zero player recovery resets and no script errors; Home's explicit retirement
notice is recorded as informational.

Earlier Beach approaches collided with the stilt-house/boardwalk footprint;
an initial Winter path missed ice marks; an early reward approach hit the
correctly solid open door leaf. These test inputs were corrected and were not
counted as passes. A Full request for Static-only Arcade was correctly refused;
that positively identified owned test child was stopped, its logs retained,
and the next run used supported variants. No game or map regression was found
in this follow-up, and no rebake or rebuild was needed. The earlier cleanup
remains valid; subsequent approved feature work may recreate build output.

### Capture-free performance and pacing

Every run has VSync off, no frame limiter, real wall-clock delta, 240 warmup
frames, the same final player SHA-256 below, actual 1920x1080 drawable and
**NVIDIA RTX 3070 Laptop GPU / DX12 / Immediate** diagnostics. Textures and
filtering are High. Menu/Pool use High lighting, Full lightmaps/reflections and
bloom. Arcade/furniture are deliberately labeled **Static**: the runtime reports
Low lighting, lightmaps off and reflections off for this supported variant.
Those two are not Full-quality performance acceptance.

| Native scene | Average presented FPS | Mean loop ms | p95 loop ms | Worst loop ms | Ready presents / missed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Menu, High / Full | 144.61 | 6.915 | 14.191 | 15.930 | 1441 / 0 |
| Pool, High / Full | 179.96 | 5.557 | 11.469 | 14.205 | 1797 / 0 |
| Dense Arcade, Static only | 174.16 | 5.742 | 9.680 | 12.757 | 1733 / 0 |
| Furniture, Static only | 155.04 | 6.450 | 8.961 | 11.051 | 1560 / 0 |

The window initially configures Fifo, then the actual applied mode is Immediate;
raw logs retain both. A **separate presentation-only** menu control reaches
317.87 FPS (3.146 ms mean loop), above the 165 Hz display refresh, with 3134 ready
presents and zero misses. It is not normal gameplay FPS. Pool/Arcade also exceed
165 Hz, while varied per-frame distributions retain presentation/GPU scheduling
cost. This establishes that the requested path is not simply pinned to refresh;
it does not prove exclusive display ownership or eliminate all hybrid-GPU/
compositor effects. No Mac-vs-Windows, AMD-vs-NVIDIA, before/after speedup,
percentage saving, or universal performance claim is made from these short runs.
Raw metrics live in `timing-*-state/receipt.json` for menu and `timing-*/receipt.json`
for maps; captures and the short media run are separate.

### Deliverables, remaining acceptance and transport limits

The complete tested map-free developer payload is `Places-Windows-0.13.0/`
(three native executables, SDL3/DXC runtime DLLs, sibling runtime/editor assets,
docs/licenses; 1,491 files, 285,036,498 bytes before any added local notes).
All eight release executables plus DLLs also remain in `preserved-executables/`;
older comparison executables remain in `preserved-initial/` and
`preserved-before-export-fix/`. Unique exports, editable projects, saves and raw
receipts live outside `target`.

Final player SHA-256:
`f0f4ea6fad9e3691fff64ffc4e69b288e051a57dd0d82dad85adeff76a277525`.
Final editor SHA-256:
`aed6ae9c1516dae33f8d8f7c4224ce6070befd47ab328fee7faae277494146b7`.
Final compiler SHA-256:
`5313c367e743c0a4eb3eddd7dd2b304d6576fb12b38352f6ef7c4b6777198f06`.

`media/windows-candidate-pool-throw-sampled-10fps.avi` is a 33-frame native
candidate-only MJPEG AVI, 1280x720, sampled at requested 100 ms intervals. Capture
overhead affects pacing; it is excluded from timing. It was encoded using
already installed Pillow, with no extra encoder installation. Exact source
frame hashes and requested timestamps are in `media/clip-provenance.json`.
There is no valid native Windows before baseline for the requested savings
montage; the original app did not render with FXC. No baseline or savings was
fabricated. Continuous native video FIFO support is Unix-only; ffmpeg/ffprobe
were not installed. The sampled AVI is not presented as continuous recording.

Library download helper could not materialize the prior Mac furniture package:
`Library prepare_materialize is not available`. A later upload probe for the
Windows executable was **denied before execution by automatic approval review**:
external transfer of that executable to Library lacked explicit payload and
destination authorization. Nothing uploaded, no new Library ID is confirmed,
and the rejected operation was not retried or bypassed. Parent/user approval
is needed before that external executable delivery. Local deliverables remain
usable and preserved.

Remaining Windows acceptance is explicit: full lighting dense
Arcade/furniture/polished package, long-duration/driver-RSS retention, continuous before/after
montage and an installer. No Linux result follows from this Windows pass.
The native focused gates above passed; this is not an unqualified full-release
sign-off. Cleanup and final branch publication are recorded in the local final
receipt after artifact preservation.

## Pool, menu and shared dependency follow-up — 2026-10-21

This follow-up supersedes the earlier menu cast, 120-beat/611-second campaign,
decorative copy, executable/package hashes and upload-helper blocker. Historical
sections retain their actual measurements and provenance. No release, tag,
version bump or main merge was performed. Hosted CI was not queried or awaited,
as explicitly instructed; it is a future version-final/pre-bump gate.

### Accepted behavior and shared fixes

The original late pickup attached the item after the visible lift. The shared
carry transaction now transfers the existing physical item at hand contact,
preserves its exact world pose at transfer and blends continuously into its
authored grip. Putdown and ragdoll release preserve that same identity. Native
parcel and duck captures show actual palm support; the original parcel key is
`9223372036854775819` and duck key `9223372036854775825`. The menu removes the
repetitive book/towel/cushion/gift handling variants, so those removed events are
not claimed as additional final grip acceptance.

The live menu now contains **seven complete events**, shuffled as whole events
without repeats within a bag. Normal fresh entries vary the seed and first event;
explicit developer seed/first-event controls reproduce tests. Settings do not
reshuffle an active interaction. The ordinary cast starts behind genuinely
occluding side wings, enters through side doors, acts and exits out of view.

1. Office parcel delivery: one NPC places the parcel and leaves; a different NPC
   enters through the other doorway and collects that same parcel.
2. Pool duck: one NPC releases the duck into real water/current, walks around the
   dry deck, retrieves it at the opposite bank and exits through a different door.
3. Home closet: an NPC opens the closet, raises both arms in fear, turns and runs;
   a full-size ghost emerges after the turn and chases them out of view.
4. Arcade play: cabinet interaction, practical-light feedback, celebration and exit.
5. Blackout: four irregular pulses over 2.65 seconds, about one second dark, then
   Arcade → Home → Pool → Office restoration. Eight roaches already occupy the
   Office floor before restoration; they scatter rapidly off screen and despawn
   only outside view. They do not queue through doors.
6. Five Arcade balloons drift with varied heights, spacing, bobbing and arrival;
   all leave view and despawn before another event needs those paths.
7. Vanishing bodies: seven outward-opening side doors admit seven NPCs, which
   become real articulated ragdolls. Flicker leads to full darkness; only then
   are all bodies removed, and flickering restoration reveals an empty scene.
   This event alone deliberately permits a crowd.

The old ten-minute/120-beat target was superseded by quality over quantity and a
15-event cap. Two complete final seven-event bags reset at 171.806 and 341.639
seconds in the deterministic native campaign. Every temporary entity returned to
the six-character baseline; diagnostics stayed empty. Balloon ownership was
corrected after a repeated-cycle test exposed hidden permanent balloons blocking
later crowd entrances. Ghost emergence and the duck's final facing/arrival were
also corrected from native failures; failed attempts remain in local evidence.
The wordmark remains the committed font atlas. Under-title and top-right
slogans are removed. Left-side light banding came from sixteen constant-alpha UI
strips: one continuous-alpha quad using the existing white PNG removes the seams.
Native standard/alternate styles, three aspect ratios and small UI captures were
inspected. This is not a universal artwork-quality claim.

The Pool thrown-item case required additional shared work beyond excluding
unrelated doors and props. Source/cell caustics certificates, receiver keys,
immutable ray reuse and hoisted room/floor probe guards reduce the matched box
impact CPU update **5.779 → 2.035 ms**. Prethrow/impact/settling/recovery means were
**0.596/5.779/1.839/0.501 → 0.836/2.035/1.251/0.671 ms**: the later idle and recovery
samples are higher and remain explicit. Mug and a Home cushion exercised the
same infrastructure; static queries stop after settling and unrelated Home props
retain their samples. There is no Pool/model allowlist or future-asset opt-in.
The per-dynamic-ray experiment was ineffective and removed, with its patch kept
locally. The user reported the Pool symptom resolved; this is not represented as
an independently described personal acceptance test.

### Focused ragdoll and dependency review

Ragdolls use the ordinary character receiver/probe path with posed bounds, and
pose revisions change only when physics actually changes bones. In the focused
seven-ragdoll fixture all 105 bodies slept, with **zero additional pose revisions,
lighting refreshes or immutable queries** across the stationary checkpoints.
Contact wakes the target while the other six retain cached lighting; reset retires
all 98 joints and bone-pose caches. Unrelated rigid props and water perform no
additional lighting queries during the ragdolls' motion. Existing character
projected grounding/flashlight shadows remain their implemented contract; this
review does not introduce skeletal ambient mesh occlusion or claim it existed.

Separate baseline stage samples measured active character lighting about
0.083 ms for one physical actor and 0.504 ms for seven. Seven sleeping actors
used 0.0012 ms for lighting, 0.0096 ms for pose publication and 0.0192 ms for
skin/upload checks. The same fixture keeps seven visible actors: six ordinary
idle actors in the one-ragdoll case exposed an independent defect. Automatic
zero-duration poses advanced revisions and re-skinned unchanged vertices. Shared
animation now detects exact unchanged local transforms/morphs before composing,
updating joint deltas or invalidating GPU skinning; real keys, morphs, crossfades,
cue release and waking still update. Focused regression uses one synthetic node.

The queued dependency pass preserves separate old/new bounds per changed caster,
including rotation/scale and spawn/despawn. Two distant changes no longer form a
large enclosing dirty box that needlessly refreshes receivers/water between them.
Rigid, character, ultraviolet and caustics consumers use the shared regions.
The list is bounded by the existing scene budget and released with the world.
Missing history, changed resources or incomplete ray caches conservatively refresh.
Energy-only fixture switches retain immutable visibility rays while recomputing
illumination; a missed resource revision cannot adopt this shortcut. Prepared
probe bounce and potentially global source-energy effects remain conservatively
resampled where narrower dependencies cannot be proved. No lighting is frozen,
resolution lowered, effect removed or map-specific exemption introduced.

Eight-probe switch fixtures compare every cached HDR anchor/direct tap exactly
against uncached refresh. Small water fixtures compare exact samples against full
refresh for movement, rotation/scale, entry/exit, removal and missed revisions.
The 2/16/64-caster cases measure the former enclosing-union work against separate
regions. Final measured results and exact executable identities are retained in
`corrections/dependency-tracking-results.json` and `corrections/FINAL-ACCEPTANCE.json`.

The final native source-switch runs add **zero immutable queries** on both off
and on transitions, change actual illumination and restore the exact prior HDR
anchor payload. Six ordinary idle actors retain revision 1. In the matched
steady-menu control, CPU update time falls **0.231 → 0.090 ms** at 3024×1676;
observed FPS is 149 → 206, with macOS pacing variation explicitly preventing an
uncapped-throughput claim. The 2/16/64-caster water fixtures reduce **726 → 0**
cell queries, preserving exact full-refresh output. Small diagnostic CPU times
were 71/154/316 → 1/2/10 microseconds; these ran during compilation and are not
native FPS results. Active ragdoll costs vary with substep count/presentation:
final one/seven-physical-actor windows average 1.246/2.503 ms total CPU update and
0.243/0.726 ms physics step; all-bones-asleep windows average 0.533/0.474 ms total
update and 0.046/0.065 ms physics. Both cases retain seven visible models. Final
short active lighting/skin profiles are 0.148/0.400 ms for one and 0.570/0.979 ms
for seven. The earlier samples are retained rather than claiming every timing
improved; zero unnecessary stationary/unrelated queries is the acceptance result.

### Validation, distribution and evidence

Formatting, strict debug/release Clippy and all **254 Rust tests** pass; the locked release workspace/all-feature build passed in 195.59 seconds. Final package identity is recorded in the local handoff.
The source Python suite retains **363 passing and three optional native skips**;
asset validation retains **621 assets, zero warnings**. No full shipped-map bake
was added to Cargo tests or automatic CI. Native integrated assertions pass 36
checks, and installation tooling passes 27 cases. The editor round trip saves,
reopens, compiles, plays, returns, exports and imports outside the checkout with
exact JSON/Luau recovery. Empty gameplay-map startup still has the bundled live
menu and preserves gameplay lighting preferences. Master mute/persistence,
Explore/pause/return, styles and audio shutdown passed; four archive campaign
launchers and all 14 packaged members are discoverable outside the checkout.

Capture-free menu windows at actual **3024×1676**, Full/high/bloom/reflections,
VSync and limiters off, actual Immediate, observed roughly 236–253 FPS for ordinary
events and 120 FPS for the crowd. Discrete refresh-related macOS presentation
states differ between launches: these are **not established uncapped throughput
comparisons**. A separate native profile's 96 selected first/slow render traces
averaged 4.627 ms, including 4.202 ms surface acquisition, 0.326 ms encoding and
0.050 ms submission. These selected trace means are not all-frame utilization;
GPU spans overlap and include scheduling waits. The older 147.75 FPS sample used
an earlier scene at 1600×900 and is not a matched before/after result.

The separately named macOS candidate preserves existing packages. Its 14 curated
members retain their accepted historical preparer fingerprints; final-runtime
compatibility is exercised without relabeling them as newly rebaked. The menu is
current-source verified. Archive hashes, signature and extraction custody are in
`corrections/archive-receipt.json`. The final separate archive is
`Places-0.13.0-macos-arm64-dependency-candidate.zip`, **715,574,566 bytes**, SHA-256
`56da16f611f81c9404851292fc4459d788990de2f506a001f9f71979eaf73beb`;
its strict ad-hoc signature and all 1,502 extracted file hashes pass. The final
standalone player SHA-256 is
`16c07cb27ce5452aba5a749737b511a0488c7c6ed0593d6ef70cb1c4fad4f0ec`.
Native Windows/Linux, clean-OS installation,
downloaded Gatekeeper/notarization and standalone compiler GUI interaction remain
unverified. The GUI control timed out; no GUI bake or security bypass was sent.
CLI and native editor compiler paths passed. Historical **+312 MiB** residency and
independent presentation pacing remain unresolved, although map owners release.
Straight stairs retain their overlapping-flat-floor limitation and 0.15 m backing;
Annex has zero errors/29 current heuristic warnings (48 historically). Ragdoll
reset is explicit; historical transient grip overlap is not falsely marked fixed.
Architecture #31, player/multiplayer 0.14 and game modes #25/#26 remain deferred.

Delivery recovered through the supported direct local-file Library tool after
historical `prepare_uploads` failures. **Thirteen media files have confirmed IDs** in
`corrections/library-confirmed.json`: six editor screenshots, stair video, original
ragdoll clip and five final menu files including the smaller delivery copy. The single
[seven-event video](https://chatgpt.com/api/library/files/libfile_726acb1e7878819198ed8d8f7970ec3f/download)
is 338 seconds, 1280×720 H.264, 2.07 MB at 2 fps, with all 676 original frames
retained. Its original 1600×900 copy remains preserved: Library accepted it,
but sending that 29.9 MB file to the main chat exceeded the ChatGPT attachment
limit. The smaller copy was confirmed delivered before work resumed. Separate
roach, duck and vanishing-body clips retain 10 fps. The main archive remains local without an invented Library link.
All original indexed artifacts, source recovery, report originals and raw failed
attempts remain outside target. Cleanup occurs only after final local validation,
remote verification, custody checks and no active target users; the installed
post-clean launch and explicit ownership release are recorded in `FINAL-HANDOFF.json`.

## Integrated checkpoint before Pool/menu follow-up — 2026-10-21

This checkpoint records the integrated validation and distribution before the
later user-requested thrown-prop and menu corrections. Its executable, menu and
archive hashes are historical; the final correction section and handoff identify
the accepted replacement bytes. No release, tag, main merge, deployment or version
bump is authorized. The latest user instruction removes hosted CI waiting from
this task; it remains a future version-final/pre-bump gate.

### Reports and final source checks

There is one canonical report for each version from 0.7.0 through 0.13.0, listed
in the [report index](README.md). Ninety-four former feature/stage/investigation
reports were integrated: eighteen into 0.7.0, twenty-five into 0.8.0, thirteen into
0.10.0, fourteen into 0.11.0 and twenty-four into 0.12.0. The 0.7/0.8 records retain
their historical dates and package metadata; no new release or tag is implied.
Unique substantive implementation, measurements, limitations and evidence remain;
superseded queue instructions are retrospective. Authoring, API and developer
guides remain in place, and raw CSV/images/original local evidence are preserved.
The two unrelated untracked Python audits are unchanged. `report-mapping.json`
and `reports-before/` retain the exact inventory and original bytes outside target.
All local Markdown paths and anchors pass.

Compared with baseline `b7459119`, tracked `docs/` files fall **429 → 338** and
tracked `docs/reports/` files **100 → 9**, a net reduction of 91. The report subtree
now contains seven canonical Markdown reports, one index and one raw CSV. Excluding
the index, report Markdown documents fall **99 → 7**. The 94 source reports were
replaced by two new historical canonical reports and one index; the existing five
canonical reports remain. Local backups and the untracked audits are excluded
from these tracked counts.

The baseline CI failure on `b7459119` was an engine test including optional
`projects/relay-annex/scripts/relay_intake.luau`, while CI deliberately excludes
projects. The existing recovery test now uses a minimal local Luau fixture and
keeps its progress/reward/group-isolation assertions. Its scope is the shared host
contract; production project checking and native play cover the authored script.
No Rust tests were added at that initial checkpoint and no map bake was added to Cargo or CI.

The first source Python run found obsolete constructor goldens after accepted
model/art adoption. Six redundant tests tied to superseded byte layouts were
removed; useful source-geometry, topology, contact, asset and clip checks remain.
The delivered ghost cat no longer satisfies its historical COLOR_0 construction
heuristics. Accepted Aero native atlases now require exact ledger model identity
and standalone/embedded PNG parity; the accepted cockroach verifies its ledger
identity and GLB structure and refuses constructor overwrites. No GLB or PNG bytes
were changed. The packaging fixture now includes the required editor preview
folder. Native installation tooling waits for actual ready-world wall-clock input,
capture and quit, and expects the current actionable missing-dependency refusal.
Earlier failed attempts and their evidence are preserved.

| Gate | Final result |
| --- | --- |
| Formatting and locked all-target/all-feature source check | Passed |
| Strict workspace/all-target/all-feature debug and release Clippy | Passed with `-D warnings`, no lint policy reductions |
| Essential Rust suite | 245 passing, no failed/ignored cases or shipped-map bake |
| Locked release workspace/all-feature build | Passed; initial build 194.702 s; final incremental gate passed and preserved executable hashes match |
| Source Python | 366 total: 363 passed, three explicit optional skips, 120.547 s |
| Asset catalog and maintained generators | 621 assets, zero warnings; generator checks passed |
| Production Luau | Eleven bounded refusals and four finished-project checks passed, no map bake |
| Native integrated assertions | 36 core plus 22 additional native/package assertions passing on the final production executables |
| Hosted CI | Source checkpoint `5fcb8514` passed [run 39614334703](https://github.com/csd113/Places/actions/runs/39614334703); final tooling checkpoint `a7a5b2d4` passed [run 39620501057](https://github.com/csd113/Places/actions/runs/39620501057). Later CI is not awaited under the user’s superseding instruction |

This Mac uses the documented `RUSTC_WRAPPER=` override because its restricted
sccache cannot start; Rust/Cargo are 1.99.1. The native host is Mac14,9 / Apple
M2 Pro / 16 GB / macOS 27.0.1 (26A434), arm64, Metal. Hosted source CI on macOS
does not establish Windows/Linux execution or physical GPU acceptance.

### Integrated native acceptance

The final editor/browser campaign verifies bounded actual-asset thumbnails,
pagination/search/no-match and material controls. A copied two-member project
reports Workshop as metadata-refresh while Second stays current, refreshes only
Workshop using verified prepared geometry/light/probe/collision reuse, then
reports both current. A later chair edit marks only Workshop for baking. Build
and checker cancellation preserve the last published package SHA-256
`b393520e4264af618377854dcf5644d868898887d398794a4921e5e30f17757d`.

The ordinary W/S stair route reaches eye Y4.95 on the 3.35 m upper landing and
returns to Y1.6 with zero void resets and noclip off. Structural-side captures and
current package checks retain the solid backing/landing geometry. Annex source
is unchanged in finalization: the current checker has **zero errors and 29
heuristic warnings** (14 missing-wall, 12 reversed-face, three room-leak), with no
suppressions. The editor stage's zero errors/48 warnings remain its historical
result. The intervening shared structural-edge fix removed duplicate upper slab
bands; the remaining warnings are not an exhaustive containment proof.

Final collection replays assert the runtime member-manifest SHA against the new
package, rather than only a matching level name. Home pickup/release preserves the
real cushion identity. Pool traversal covers submersion, resurface, rim exit and
reentry with zero void resets; moving-door/contact entry returns to the starting
corridor. A tiny authored current fixture moves the player east with actual fluid
velocity 0.8 m/s, then walks its sloped bank to dry floor without a water-jump or
reset. Relay Discharge swim/ramp exit produces finite audio and releases voices.
The original Annex upper-ramp trigger now reaches eye Y11.2 on solid support with
zero resets. Earlier probes that accidentally selected the older bundled collection
are preserved and explicitly excluded from final acceptance.

Final native ragdoll contact cases sleep all fifteen bodies for wall, stair and
table trials, with fourteen joints and zero sampled settled penetration. Carry
release preserves item identity and removes its socket attachment; explicit reset
releases joints and bone caches. Eight rigs own 122 solver bodies/112 joints,
paused bone positions remain exact, and single-step advances once. Real unload
records zero bodies/colliders/joints/ragdolls/caches, zero active prepared bytes,
zero scripts/VM bytes and zero retired prepared owners. Menu return contains no
ragdolls; reload restores the authored world. The previous independent long
settle/performance and 24-activation/reset campaigns remain applicable to the
unchanged production code. Explicit reset is not automatic stand-up; the recorded
9.25 cm transient grip overlap remains an explicit limitation.

The live-menu campaign completes two Explore → gameplay → pause/settings → return
round trips, empty Explore and small-window layouts, credits and style selection.
The displayed version is 0.13.0. Master zero persists while effects/ambience remain
0.8/0.6; restart captures 266,240 PCM samples exactly zero. Shutdown has no live or
queued voices or audio errors. At this earlier checkpoint, the scene.bin hash preserved the prior
611-second/120-beat reset, five successful carriers, nineteen handoffs, fourteen
putdowns and bounded ownership evidence; a new full ten-minute recording was not
needed then. The later sparse choreography supersedes this cast and timing. The committed font-atlas wordmark and diagnostic-video limitations remain.

All new integration runs are functional diagnostics, often concurrent with the
explicit compiler export. Their frame summaries are not FPS comparisons. The
accepted performance campaigns remain the separate capture-free, V-Sync/limiter-off
runs with actual Immediate mode, physical drawable and pacing qualification. The
Pool CPU cause was unrelated moving doors/props globally refreshing water lighting:
accepted door update cost fell about 16.9 → 1.7 ms and entry/contact about
11.6–11.7 → 1.2–1.7 ms with exact visual parity. macOS presentation pacing remains
separate and unresolved. Historical +312 MiB residency remains unexplained even
though map-owned systems release. Editor Pool UI upload blocking fell 1595 → 79 ms;
total readiness stayed about 2.83 → 2.86 s. These limits and flat/regressed
measurements remain explicit; final acceptance does not claim every performance
or memory question is fixed.

### Distribution, delivery and remaining coverage

Final package/archive identities and all current member checks are in
`package-index.json`, `archive-final-receipt.json` and the final handoff. Candidates are
created separately from preceding stage distributions. All fourteen curated
members pass current-source verification and archive integrity. Exact declared
source recovery passes for all four projects. Separate lab and workshop packages
also pass; the 29 export/validate/current gates are in `package-gates-final.json`.
The full collection export is explicit developer work, outside Cargo/CI; it took
4,139.116 s on this Mac and establishes no before/after throughput claim.

| Candidate | Bytes | SHA-256 |
| --- | ---: | --- |
| environments.placesmap (nine levels) | 455,058,040 | `b798141bce7b4c682dba71cd44b35f76dd20d54ea1437745195f0628e0c86c6a` |
| relay-annex.placesmap (three levels) | 23,415,155 | `3858aa5de1def5c3cec5f0cd8aa199165dc186bdfdcbfad33ac84664d8f4d2d1` |
| closing-time.placesmap | 33,957,286 | `7d303b20e0d1805cf145232aa6e587eb92040028587f378e3e8160fb92a39cbe` |
| beam.placesmap | 6,235,122 | `ca43e27b7e38958ff4ea9fcd4926cd32a726fb7622e42a1069756bf39d1c04fa` |
| macOS arm64 final candidate ZIP | 706,535,175 | `e3eddb31e66f77525e165eefef096334e345d3023496892b98a95483ff7cc153` |

The ZIP is extracted into a path containing spaces and Unicode outside the
checkout. Every extracted file hash matches, the app signature verifies, and all
four campaign launchers run their expected world and controls. The extracted app
also loads Pool's shared/map resources and completes its entry/contact return with
no asset override. The final guide refresh preserves all 1,492 flat runtime files
byte for byte, updates README/installation text to the actual bundled-menu policy,
and repeats signature and extracted-file checks; the preceding ZIP remains intact.
This is a local candidate, not a published or notarized release.

Actual first-run/import
acceptance passes all 27 checks: empty live menu, writable folder setup, override
precedence, inbox import, duplicate/corrupt/conflicting packages, missing resources,
restart from another cwd, read-only installation, unavailable storage, Unicode
paths and exact settings/save custody. The historical task-1 fixture is preserved;
only a private copy's comment-only placeholder was adapted to the current empty
Luau-module contract.

A copied final editor runs from `/tmp/places-final-0130-5fcb8514/Places`, cwd `/tmp`,
with no asset-root override: edit → save → exit → reopen → build → sibling-player
import/play → return → export → exact JSON/Luau recovery all pass. The copied
map-free player presents the live menu and empty Explore from that installation.
Flat and .app metadata are 0.13.0; strict ad-hoc signatures and relocated SDL3
checks pass. The bundled menu scene is **8,022,170 bytes**, SHA-256
`e582c5e5cef1f360fcc1a61fae09847c0df78f93554aa51c599443b2fb3eb954`, and does
not depend on gameplay maps. Current acceptance supplies functional startup and
component size; it does not claim an isolated before/after startup-latency gain.

Windows/Direct3D 12 and Linux/Vulkan native build/runtime, Linux VM installation
and display-session checks, physical GPU coverage, clean-OS installers,
Developer ID/notarization and downloaded Gatekeeper trust remain unverified. The
repository exposes macOS source CI and no connected Windows/Linux runner/device.
Issues #17 and #21 stay open with these gaps. Cross-compilation or static checks
are not substituted for native execution.

The standalone final compiler GUI launcher executed, but its native UI
control/read tool timed out after 1375 seconds. The parent reported an approval
request; its resolution was not confirmed. No GUI bake/control action was sent,
no uncertain operation was retried and no approval/denial was bypassed. CLI builds,
installed compiler workflows and native editor build/reuse/cancel paths pass;
interactive standalone GUI coverage on this final candidate remains unverified.

`deliverable-index.json` preserves 35 earlier requested files / 102,340,418 bytes,
including editor screenshots/stair video, complete menu evidence and ragdoll clip
and editable export, with exact local paths/hashes. Final candidates and binaries
are indexed separately. The prior Library preparations repeatedly failed with
`prepare_uploads failed` and returned no confirmed IDs. Current supported tools
still require the same prepared route; no materially different legitimate recovery
was available and no identical retry was made. These local files are preserved,
not represented as delivered attachments or invented download links.

All evidence, source recovery, requested media, executables and candidates remain
outside target. Authorized cleanup follows completed Rust/build/native/package
work and a no-target-users check. Hosted CI is not a gate under the latest user instruction. `cleanup.json` and the
final handoff record the actual cargo-clean result and post-clean preserved launch.
Unfinished platform/delivery coverage does not authorize the deferred architecture
#31, player-model/multiplayer 0.14 work or game modes #25/#26.


## Stage 1 — baseline and reproduction

Completed 2026-10-20 on `0.13.0-staging`, baseline source `02120c79`.
This stage records existing behavior and measurement limits; it fixes no game
bugs. Stage 2 should begin with targeted core-bug reproductions below. Model
rebuilding remains paused. No release, main merge, map bake or asset edit was
performed during recovery.

### Evidence and measurement conditions

Raw evidence remains outside `target` at `local/stage1-0.13.0/`: `checkpoint.json`
contains metrics, `checkpoint-evidence.json` contains selected native action/body
events, and each case retains settings, environment, native log, frame/player CSV,
load trace, identity and resource receipts. Diagnostic cases also retain physics
traces and captures. `compiler/report.json` and `package-storage.json` retain
compiler commands/hashes and storage accounting. These local files are not part
of this tracked report; preserve them on this Mac for subsequent stages.

- MacBook Pro Mac14,9, M2 Pro (12 CPU / 19 GPU cores), 16 GB unified memory,
  macOS 27.0.1 (26A434), Metal; Rust/Cargo 1.99.1.
- Direct native preserved player `local/release-0.12.0/source-binaries/places`,
  SHA-256 `89691692caff33853ef02cf70610d28a4e1883519bb40b112c49bdd21f3e4908`.
  Only release documentation changed between its build source `7240e2f9` and
  `02120c79`; stage 1 changes only developer tooling/reporting.
- Windowed **640×360 logical / 1280×720 physical drawable**, High texture quality,
  filtering and lighting, Full lightmaps/reflections, bloom on, 60° FOV.
- **V-Sync off, no frame limiter in successfully presented gameplay**. Logs show
  the actual switch to Immediate and BENCH `swap_interval=0`; the initial Fifo
  renderer line precedes settings application. Accepted runs have zero missed
  ready surfaces. FPS exceeds the 120 Hz display rate and many frame times are
  below 8.33 ms, establishing uncapped throughput rather than a refresh plateau.
  Compositor scheduling remains visible in render/loop tails.
- Static viewpoints use fixed 1/60 simulation delta; explicit `--real-delta`
  reproductions use wall-clock movement. Item physics uses its normal clock.
  Match environment/settings and trajectories before comparing later changes.
- Recovery checked all 15 saved CSV summaries and runtime member-manifest
  identities against native logs and preserved packages, plus drawable/presentation
  evidence. All four storage reports exactly match read-only recomputation.
  No game/compiler measurements or passed builds were repeated.

### Throughput and resource baseline

FPS is `1000 / mean(loop_ms)`. CSV percentiles use nearest rank. GPU means
deduplicate sample IDs; GPU spans include scheduling stalls and are not GPU
utilization. CPU stages below are wall times, not CPU utilization. Peak RSS is
the whole process high-water mark, including startup/loading.

| Saved case | Frames | FPS | Loop mean / p95 / p99 ms | CPU update / render mean ms | GPU mean ms | Peak RSS MiB |
| --- | ---: | ---: | --- | --- | ---: | ---: |
| hub-r1 | 240 | 202.43 | 4.940 / 14.735 / 17.509 | 0.225 / 4.380 | 1.174 | 437.3 |
| home-idle-r1 | 900 | 208.06 | 4.806 / 13.411 / 15.768 | 0.334 / 4.137 | 1.203 | 473.2 |
| pool-far-r1 | 900 | 172.28 | 5.805 / 14.585 / 16.629 | 0.330 / 5.145 | 2.649 | 512.0 |
| pool-near-r1 | 900 | 166.43 | 6.008 / 13.723 / 16.542 | 0.286 / 5.451 | 2.093 | 514.0 |
| arcade-r1 | 900 | 175.02 | 5.714 / 14.425 / 14.824 | 1.366 / 4.164 | 2.503 | 1266.9 |

These are short, single-run baselines without confidence intervals or thermal
controls, not optimization gains. Pool far/near are different authored views.
GPU utilization and driver allocation counters were not collected. Unified
memory means RSS and inferred GPU allocations must not be added as independent
physical pools. Home's native atlas report is 67,108,864 bytes including its light
state; the inspector's 33,554,432-byte base-page shape is not total residency.

Action/capture/physics-trace runs are diagnostic and include their stalls. Home
cushion/carry/books cases averaged 205.26/194.50/172.02–183.67 FPS without a
sustained severe collapse. `pool-release-r1` averaged 85.83 FPS (loop p95 17.970 ms,
CPU update mean 9.814 ms); its interaction/capture workload needs phase profiling
before assigning a cause or comparing it to steady viewpoints. Ramp correctness
evidence failed the scene-submission timing gate and supplies no accepted FPS.

### Reproductions and stage 2 targets

| Report / saved case | Observed behavior | Remaining targeted work |
| --- | --- | --- |
| Relay ramp fall-through (#27), `tower-ramp-r1` | Far rising ramp x15.5–18, z2–8, offset 6.4 / rise 3.2. Start eye (16.75,8,1.65), yaw180, ordinary forward and jumps at 0.9/1.8/2.7/3.6 s after ready. Eye falls below support (y8.117,z4.950 at 2.57 s), reaches y−2.065, then void recovery increments reset_count to 1. | Isolate ramp support/contact failure with a small fixture; do not assume the stairs share its cause. |
| Pool dormant contact (#27), `pool-contact-r1` | Forward/jump into wet-floor sign near (18.65,0,14.5): eye y1.707,z14.876, all 19 bodies stay dormant, forward progress stops. Backward input escapes to z15.55. | Compare already-active prop contact and escape paths. Failed activation is observed; a persistent trap is unproven. |
| Relay technician walking in place (#28), `tower-npc-r2` | Ground-floor tower_tech remains at (3,0,4), automatic clip pose_stand → walk while player walks → pose_stand on stop; native idle/walk/stop captures retained. | Isolate fallback to player locomotion in character advancement. Upper technician/witness have explicit cues; do not generalize to every NPC. |
| Required supply in well (#28), `well-r1` | Ordinary E pickup; diagnostic Player placement at lip (12,1.6,13.3), then ordinary E drop. Supply sleeps at (12.179,−2.216,15.839); repeated aimed E from tested lip fails to retrieve/reset it. | Check far lip/crouch and intended recovery. Local reach/progression obstruction is reproduced, not impossibility of every recovery route; player well_catch and whole-visit reset are distinct paths. |
| Second-room ceiling (#29), `tower-ceiling-r1` | Native `second-room-ceiling.png`, eye (3,4.8,6), yaw0/pitch70, shows severe black striped/coplanar artifacts. | Obtain geometry/export checker evidence before choosing a fix; no rebake establishes the cause. |
| Home cushion/books slowdowns | Pickup/drop and moving carry were exercised without sustained collapse. Book stacks were diagnostically placed on the bed and jumped onto; books end sleeping. | Confirm exact book/body support contacts and original collision trajectory before declaring trigger coverage. Neither fixed nor disproved. |
| Central stair failure, `tower-stairs-r1` | Forward + four jumps reaches south landing eye y4.8,z9.7, reset_count0. | Side/tread-edge, descending and variable-time paths remain untested. Successful tested ascent does not disprove the report. |
| Raw RELEASE BLOCKED HUD text | Bin pickup/drop near closet shelf succeeded in `pool-release-r1`; raw diagnostic emitter is present in `src/game/app/item_runtime.rs`. | Reproduce an actual blocked release and its HUD text; attempted capture name is not proof of triggering the fault. |
| Long offscreen error text | Source review found two-line menu status and fit_text in small_line. | Native long-error reproduction and other message surfaces remain unvalidated. |

Scripts `tower-ramp-actions.json`, `tower-stairs-actions.json`,
`tower-npc-view-actions.json`, `well-actions.json` and per-case environments are
retained in the local evidence root. Diagnostic placement is explicit above;
these are not claims of an entirely ordinary user traversal. Failed setup
`hub-probe` (wrong member ID) and `tower-npc-r1` (AimEntity before entity frame)
were corrected in subsequent cases and establish no game defect.

### Storage and compiler controls

Collection `assets/levels/environments.placesmap` is **449,857,977 bytes**, SHA-256
`9e78fbe3f6a2cf00f19f410da76271f1acb6c6b14d91b2e5ba0dd8682e532ee8`.
Static-only compressed payload beyond Full is **38,388,876 bytes (8.53%)**,
40 records / 101,476,379 logical bytes. Lantern Hollow accounts for 34,458,404
compressed bytes, Home 103,923, Pool 562,294 and Arcade 2,183,912. Full-only payload
is 273,487,654 compressed / 977,381,763 logical bytes; shared payload is counted
once across members. Static has distinct vertex-lit/prop recipes and switched
states, no lightmap atlas. Both modes remain preserved.

Other audited archives: Relay Annex 24,434,167 bytes (Static-only 82,412), Closing
Time 34,116,781 (44,597), Beam 6,314,015 (23,344). Costs are existing compressed ZIP
payloads including referenced PPS1 shared segments, excluding hypothetical
re-export metadata/index/header changes and external catalog assets. Per-member
totals can overlap. Installed/cache allocation was not inventoried; historical
release ZIP size 687,722,141 bytes was not rehashed in stage1.

Compiler: preserved `places-compile`, SHA-256
`8bdb4951d2ef69ce35c57dfe9abeb7897df48c25db1aaca17b78f745c9985c4b`,
`--workers 6 --variants off,full --force`, fresh process/package per sample,
120-second bound, no concurrent owned build/runtime/bake.

| Tiny existing fixture | Wall samples s | Median wall / CPU s | Median peak RSS MiB |
| --- | --- | --- | ---: |
| test_room.json | 8.070 / 7.804 | 7.937 / 38.580 | 278.2 |
| material_transport.json | 6.509 / 6.429 | 6.469 / 31.724 | 322.2 |

test_room Full prepare/encode was 7.549/7.609 s versus Static 8.451/7.833 ms;
material_transport Full includes offline reflection capture. These controls do
not estimate shipped collection bake time or editor latency. Cache-hit/reuse
timing and deterministic output equality remain gaps for the storage/compiler
stage, not reasons to repeat this baseline campaign.

### Tooling review and delivery scope

`tools/bench/frame_time.py` adds explicit package/member identity verification,
wall-clock interaction option, physics diagnostics, mean/FPS and process CPU/RSS
receipts. `tools/bench/package_storage.py` is a read-only trusted-output inspector
with bounded metadata reads, shared-segment accounting and explicit caveats; it
is not an untrusted-package validator. Two tiny ZIP/log identity regressions in
`tests/test_compiler_bench.py` protect acceptance of the planned member/content
and variant without baking or GPU cost. No Rust tests were added.

Saved validation in `tooling-validation/status.json`, `review.json`,
`python-checks.log` and `offline-review.json` confirms format exit 0 (1.51 s), strict
workspace/all-target/all-feature Clippy exit 0 (24.97 s), locked all-feature release
build exit 0 (88.06 s), Python syntax and 37 relevant tests passed (0.673 s).
Recovery verified all three tooling source hashes still match that review;
there is no build invalidation. The unrelated Python audit/inventory reports
remain untouched. No surviving game/compiler/Cargo/benchmark writer was found.
All local raw outputs and preserved release binaries remain available; no cleanup
was performed. Remaining reproductions belong to the subsequent bug-fix stages.

## Stage 2 — core correctness

Completed on the connected Mac on 2026-10-20, continuing `0.13.0-staging` from
`f70dbd1b560a1ea759cd22fc76c7154a1bb72035`. Local HEAD and the live remote were
verified before editing and again before delivery. The previous stage was idle;
no competing game, compiler or Cargo writer was present. The unrelated Python
audit/inventory reports and all stage 1 evidence remain untouched.

The current #27/#28 descriptions, repository instructions, canonical map guide,
the Stage 1 baseline section above, checkpoint evidence, 15 native receipts and relevant
captures informed this work. The baseline matrix was not repeated. Raw stage 2
receipts, action scripts, captures, logs, summaries and direct binaries remain
outside `target` under `local/stage2-0.13.0/` on this Mac.

### Causes and changes

**Airborne ramp collision.** Horizontal terrain sweeps closed ramp perimeters,
but omitted the interior contour where the rising slope crosses the feet plane.
The airborne height query discarded the now-higher ramp and treated that space
as unsupported. Once inside the wedge, descent also discarded the surface above
the previous feet, allowing a fall through to the void. A tiny elevated-ramp
controller fixture reproduced this before the fix. The shared floor model now
emits that airborne contour and lands on the highest real slope point within the
circular feet footprint, using the circle's chord at side edges. Walking retains
its existing step rule. No map coordinates, invisible replacement floor, larger
recovery envelope or respawn workaround were added to the controller.

The reported stair failure remains less specific: neither stage 1's ascent nor
this stage's ascent, jumping descent and crouched descent reproduced it. Existing
riser/perimeter handling is retained; a small tread/side-edge landing regression
protects it while the shared terrain path changes. This is evidence for the
exercised paths, not proof that every reported stair trajectory is fixed.

**Dormant movable props.** The player controller stopped before dormant shapes,
while activation only considered explicit actions and active rigid-body impacts.
An all-dormant world then skipped solving entirely. Before that fast path, the
physics world now recognizes the player's exact shape contact, including its
small swept contact gap, and activates configured movable bodies. Fixed bodies
stay fixed. Pause/zero scale, disabled player collision, suspended bodies and the
existing player-penetration refusal remain enforced. Ordinary solver sleep after
activation remains distinct from authored dormancy. The established rule that
movable rigid props are pushed rather than controller-owned standable floors is
unchanged.

**NPC animation ownership.** Unaddressed placed characters explicitly inherited
the player's locomotion snapshot. That input is removed from the shared scene
update API. Each actor follows its own action cue, explicit locomotion input or
frame displacement; stationary skinned actors advance idle. Rigid animated props
retain their pose without an action. The player's visual representation still
receives its own simulation snapshot separately. Authored poses and intentional
stationary walk cues retain precedence.

**Required supply recovery.** Intake had a player-only well catch and whole-visit
reset, with no item-loss policy. The required box now has its own
`essential_supply` reset group. An exact-instance, unheld-item volume in the well
uses the existing atomic `reset_physics_group` action. The accessible well sign
also requests that group, providing recovery from other inaccessible placements
and retry after clearing an occupied return point. It preserves other objects
and objective state, creates no duplicate and refuses unsafe placement. After
acceptance, the sign preserves the delivered box; the existing delivery guard
prevents repeated rewards. Player falls still use the established visit reset.
This is authored policy using shared capabilities, with no per-map engine branch
or new editor/schema subsystem. Guidance and the authoring guide document it.

**Release and feedback.** `can_drop` treated every shape intersection as unsafe,
including a harmless resting overlap of ten microns. The focused fixture failed
before the change. Intersecting shapes now receive a distance check against the
existing `CONTACT_EPS` (0.1 mm); genuine penetration and failed shape queries still
refuse release and retain ownership. Camera clearance and swept carrying remain.
The ordinary HUD uses short feedback, “Make room to set this down.” or “The item
cannot follow here.” Raw diagnostic labels and the unbounded authored-item ID are
removed from that surface. Detailed body state and blocked-action events remain
in developer traces/authoring diagnostics. This fixes a demonstrated false
refusal, without claiming that every historical warning had that cause.

### Native validation

Direct preserved executable:
`local/stage2-0.13.0/source-binaries/places`, SHA-256
`7c70e8f76d4475de6fcdc0bb6e3def5e8c080d33f54e390554b2b8b025bd4497`.
MacBook Pro Mac14,9 / M2 Pro, macOS 27.0.1, Metal, High textures/filtering/lighting,
Full lightmaps/reflections, bloom, 60-degree FOV. All cases used V-Sync off and no
frame limiter, **640×360 logical / 1280×720 actual drawable**, actual Immediate
presentation and zero missed ready surfaces. Gameplay used real delta. The ramp
unit fixture additionally covers 30 Hz, 144 Hz and mixed intervals on positive
and negative slopes along both axes. Diagnostic captures and interactions are
correctness evidence; no FPS improvement or matched performance comparison is
claimed. No temporary app wrapper was used.

Starting placements and fixture manipulations below are explicit diagnostics;
walking, jumping, pickup, drop, throw and door input use the production paths.
`native-summary.json` provides compact selected events; per-case environments,
receipts and action scripts preserve the exact setup.

| Evidence directory | Result |
| --- | --- |
| `tower-ramp-r1` | Saved far-ramp start eye (16.75,8,1.65), forward and four jumps: reaches eye (16.75,11.2,9.70), feet 9.6, grounded, reset count 0. Stage 1's corresponding path fell and reset once. |
| `tower-negative-ramp-r1` | Middle negative-rise ramp: jumping ascent reaches feet 6.4 at the north landing; descent reaches feet 3.2 at the south landing, reset count 0. A separate side setup was displaced by spawn clearance near an adjoining guardrail, so it does not prove the intended side traversal. |
| `office-ramp-r2` | Correctly elevated east-shaft control: repeated-jump ascent reaches feet 7.2 and descent reaches feet 3.6, grounded with reset count 0. Existing environment package and geometry unchanged. |
| `tower-stairs-r1` | Repeated-jump ascent reaches feet 3.2; jumping and crouched descent return to feet 0, reset count 0. Outside-foot side approach is blocked safely; it does not establish an ascent along the full outer edge. |
| `pool-contact-r1` | Wet-floor sign contact changes 19 dormant bodies to 18 dormant plus one active, then sleeping. Player passes to z11.99 and backs to z13.47, reset count 0. Actual initial eye z15.55 after clearance; this is not a matched timing comparison with stage 1's z15.4 setup. |
| `tower-npc-r1`, `home-npc-r1` | Technician remains `pose_stand` at (3,0,4) during player lateral walking/stopping. Upper technician and witness retain their independent authored cues. Home caretaker moves from x5.5 to x4.55 via its navigation goal, using walk then standing, while player eye stays (5.5,1.6,6). Captures visually inspected. |
| `well-delivery-r2` | Two ordinary E pickup/drop losses into the well return the same box to (10,0,10). Ordinary delivery settles it at approximately (3.957,0.002,7.977), emits one acceptance and unlocks the airlock. Post-completion sign leaves it sleeping there. Unrelated spray bottle stays at its authored pose. |
| `recovery-retry-r1` | Occupying the return point with another prop causes automatic and manual reset refusal with identical before/after body state. Moving that blocker away and using the sign returns the box. Delivery succeeds, the ordinary door/airlock path enters Tower with empty hands, and a diagnostic reload returns to fresh Intake with exactly one required supply. |
| `release-r1`, `home-carry-r1` | A diagnostic overlapping prop in paused Pool produces a real blocked drop and retained ownership; capture shows short feedback. Clearing it permits retry, pickup and throw, then normal settling on resume. Home ordinary cushion pickup/drop/pickup/throw also succeeds. |
| `long-error-r2` | Tiny unsafe-path ZIP import is rejected by existing validation. Its long error remains within two fitted on-screen status rows; native capture visually inspected. |

The three-map Relay export is retained as
`local/stage2-0.13.0/Relay-Annex-Stage2.placesmap`, 24,070,169 bytes, SHA-256
`8c3ae62322a515d5208163f4c793341544ffa7180e3425bb770e5106bf5acbd1`.
The production compiler validated/exported the changed gameplay sources. Its
prepared-cache checks missed for all three members, so that project was prepared
once; no environment-collection rebake or geometry/art edit was performed.
Original packages remain intact. The compiler executable is preserved beside
the player, with its hash and command in the local checkpoint/export log.

### Checks and review

All required checks exit 0:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`: **198 passed**, no full-map bake; suite execution about 0.4 seconds.
- `cargo build --locked --release --workspace --all-features`
- `python3 tools/assets/validate.py`
- `python3 tools/textures/build.py --check`
- `python3 tools/props/build.py --check`
- `git diff --check`

Six focused tests add concrete protection for airborne ramp entry, stair-edge
landing, first dormant contact/fixed policy, resting release/real-overlap
refusal, actor animation ownership, and the actual Intake script's recovery and
single-reward policy. Fixtures are tiny metadata worlds or one synthetic rig,
with no renderer, texture generation, source-map baking or broad suite
restoration. Test-only interval expansion did not change the preserved production
binaries. Final logs are `*-final.log`; red/green reproduction logs remain.
The catalog validator reports zero warnings. Texture checking retains 96 existing
preferred-resolution/manifest advisories, and prop checking retains existing art
budget annotations; no visual asset was changed to silence them.

Review checked the shared ownership boundaries, tolerances, fixed-body exclusion,
pause and penetration guards, atomic reset refusal, quest identity/reward guards,
player representation, ordinary HUD and scoped source diff. No blanket lint
suppression, format/cap increase or unrelated cleanup was introduced.

### Remaining limits and next-stage custody

- The original unspecified stair fall-through remains unconfirmed beyond the
  paths above. Keep its report and stage 1 evidence; successful attempts are not
  universal proof. Broader stair/ramp authoring/export validation belongs to the
  editor stage unless a new concrete runtime reproduction identifies a defect.
- Severe Home cushion/books-on-bed and Pool closet slowdowns remain for the full
  performance stage. These correctness runs do not establish a performance fix.
- The Relay ceiling artifact is still visible in retained evidence; no general
  ceiling, editor or geometry rewrite was attempted.
- Essential-item loss handling is explicit authored policy. The well auto-returns
  this box; the sign supplies manual recovery for other inaccessible placements.
  This is not an automatic reachability solver for arbitrary pits/water/puzzles.
  Persistent campaign saves are unsupported by the existing game; section
  transition/reload intentionally starts fresh local state and was checked.
- Setup failures are retained honestly: `well-recovery-r1` released before the
  distant swept hand arrived, and missed delivery; `long-error-r1` selected a map
  instead of Import; `office-ramp-r1` started beneath the east shaft's actual
  floor. They are not passing evidence for those attempted checks.
- Water implementation, storage/compiler optimization, full performance, audio,
  UI/menu redesign and visual styles remain later stages. Cloud model rebuilding
  stays paused; Spoonerman and Windows/Linux environment work remain excluded.

Stage 2 stops here. Final integrated CI waiting remains with the final queue
stage; this handoff does not merge main or publish a release.

## Stage 3 — water motion, currents and player interaction

Completed on the Mac staging branch from stage 2 commit
`ecdd0463b858739bae0818a12efd200fb3d5e32f`. No cloud model export was
integrated, no other environment was rebuilt, and land movement parity remains
future work. Raw receipts, direct binaries, captures, action scripts and the
runnable collection remain in `local/stage3-0.13.0/`, outside `target`.

### Authored controls and rendering

Water now accepts the optional, strictly validated `motion` object:

| Control | Default | Range and behavior |
| --- | --- | --- |
| `speed` | 1 | 0..8; ripple clock multiplier; 0 holds the ripple pattern |
| `strength` | 0.35 | 0..1; ripple UV/normal amount; 0 restores the flat artwork |
| `flow_speed` | 0 | 0..4 metres/second; 0 gives still fluid |
| `direction_degrees` | 0 | 0..360 Places yaw; 0 is −Z, 90 +X, 180 +Z, 270 −X |

All four controls are exposed on the editor's existing paginated water form.
For completely static water, set `motion.strength: 0` and `flow_speed: 0`;
set independent `caustics.speed: 0` too if the receiver pattern should freeze.
A current can exist without surface ripples, and ripples without a current.
Non-finite/out-of-range controls and misspelled nested fields fail validation.

The existing committed PNG supplies the surface imagery. The renderer advects
it in world metres using the material's actual UV frame, adds restrained analytic
wave slopes to the material normal, and applies a small continuous ripple UV
offset. It keeps the authored mean plane, collision boundary, lightmap UVs and
mesh fixed. Equal adjacent controls share world phase. There is no scrolling
time wrap, per-frame mesh construction, new render pass or generated texture.
Different authored controls intentionally produce different behavior at their
boundary; authors should match controls across a continuous sheet.
Texture quality and filtering remain independent from lighting quality.

Caustics retain the existing source eligibility, RGB/intensity/tint, visibility
and receiver rules. Their PNG pattern follows the same current; their own speed
and strength remain independent. Surface motion has an independent water-node
count/clock, so darkness, disabled caustics or a missing focus texture cannot
freeze water. Moon/fluorescent eligibility was not broadened. A native GPU smoke
caught an updated storage-buffer minimum mismatch; all world/decal layouts now
share the correct header-plus-node size (128 bytes), including empty water.

### Player and object behavior

The inspected behavior reference is Valve's **Source SDK 2013 singleplayer**
commit [`77567eb4bce8d7b6b7f73e929fcde3a331c0216e`](https://github.com/ValveSoftware/source-sdk-2013/blob/77567eb4bce8d7b6b7f73e929fcde3a331c0216e/src/game/shared/gamemovement.cpp),
including WaterMove, feet/waist/eye classification, CheckWaterJump, WaterJump,
swim rise and relevant HL2 overrides. Reference files are private local evidence;
no SDK implementation was copied into Places. This is independently implemented
HL2-style behavior, without a claim of measured 1:1 parity.

`src/game/water.rs` owns water locomotion. Feet-only immersion retains walking;
waist depth selects swimming; eyes determine the underwater state. The upright
standing/crouched collision shape and eased eye remain Places' existing contract.
Forward/back swim follows view pitch, strafe stays horizontal, looking up boosts
forward rise, Jump rises, and idle input sinks. Incoming plunge momentum decays
through 3D water friction instead of an arbitrary entry clamp. Floor/head contacts
continue to constrain the body, and the former hard eye float line/bob is removed.
Waist crossings hand back to ordinary gravity, with a modest natural surface
cycle when rise is held.

Reference coefficients are friction 4/s, acceleration 10/s and wish-speed cap
`0.8 × walk_speed`. The 1.8 m/72-unit standing-hull ratio gives a 1.5 m/s idle
sink wish (before 0.8 scaling), 2.5 m/s held-rise seed and 1.25 m/s water-jump
forward speed. Places retains its 9.8 m/s² gravity and ≤1/120 s locomotion
intervals. Acceleration projects velocity onto the requested direction rather
than charging a blocked perpendicular current against the whole speed budget.
This explicit adaptation keeps a swimmer controllable along a narrow bank.

A water jump requires forward intent at waist depth, a touched real walkable
rim no more than 0.5 m above the waterline, and a clear destination. Side/back
pressure, fast plunges, full eye immersion and wall/prop tops do not qualify.
The collision-checked ballistic rise is sized for waist depth plus rim clearance:
1.5 m standing or 1.05 m crouched. It ends on landing, head contact, descent or a
two-second bound. Jump remains latched across landing. Ramps use shared floor
support; stage 2's slope/feet-disc corrections remain intact. A native sideways
current originally triggered an unintended bank jump; forward-intent contact
selection plus an approach cone fixes that case. Both 0.8 and maximum 4 m/s
side-current cases have an essential CPU regression.

Player drag and object drag share the same authored fluid vector. Five footprint
samples ease player crossings; wading applies immersion weighting. Rigid-body
flow is weighted by actual displaced submerged volume, retaining density,
Archimedes buoyancy, centre-of-buoyancy torque and stable exponential drag.
Sleeping active bodies can wake in current; held, dormant, suspended and fixed
ownership stays intact. Dense objects still sink and bottom friction can arrest
their drift. `linear_drag: 0` intentionally removes an object's current coupling.

### Source/package compatibility and deliverables

Level format remains 1 and collision/geometry record layouts are unchanged.
Validated embedded source hydrates motion alongside compiled water records.
New water exports require capability `water-motion-v1`; older players reject
that named requirement. Existing packages load with calm ripples and zero flow.
Changing only motion changes source identity while reusing the prepared-world
key; changing the mean water plane still invalidates preparation. A tiny key
regression and an actual reversed-current export verify both sides.

Pool's five existing water volumes now author restrained controls: matching
main/shelf/secondary/bank speed 0.85, strength 0.35, zero flow; hot tub 0.65/0.15.
Only Pool was prepared, in 244.419 seconds with six workers, static/full variants,
inside a 480-second bound. The standalone export preserves Pool's hub transition
and therefore needs its complete project to play. The runnable
`The-Quiet-Collection-Stage3.placesmap` combines that verified Pool member with
all unchanged original members. All eight other member mappings and eleven
other source records were verified unchanged; the repository's original bundle
was preserved as `The-Quiet-Collection-Stage2.placesmap` (SHA-256
`9e78fbe3f6a2cf00f19f410da76271f1acb6c6b14d91b2e5ba0dd8682e532ee8`).
The new collection is installed atomically at the ignored runnable
`assets/levels/environments.placesmap`; `collection-install-receipt.json` records
both identities. Production dependency validation reports no warnings and Pool
verification reports current=true, differences=[]. Composition took 12.532 s,
streams bounded chunks, and does not rebake the collection.

`tests/fixtures/levels/water_flow.json` is the manual authoring example: three
identical 2.25 m deep bays, left still, centre +X 0.8 m/s, right −X 1.6 m/s, with
matching floating ducks, dense baskets and real walk-in banks. Surface controls
also differ visibly (centre 0.8/0.4, right 1.6/0.8). It exports separately in
4.063 seconds. `Water-Flow-Reversed-Stage3.placesmap` changes only the centre's
motion to −X 1.6 m/s; its 0.2-second export reuses verified geometry, lightmaps,
probes and collision. The editor/source controls provide still water directly.

To inspect the installed Pool, run the preserved player with `PLACES_LEVEL=pool`.
For the isolated authoring example, from the repository root:

```sh
PLACES_ASSET_ROOT="$PWD/local/stage3-0.13.0/resource-root" \
PLACES_STATE_ROOT="$PWD/local/stage3-0.13.0/flow-example" \
PLACES_LEVEL=water_flow local/stage3-0.13.0/source-binaries/places
```

### Checks and native evidence

Required checks pass: `cargo fmt --all --check`, strict workspace/all-targets/
all-features Clippy, `cargo test --workspace` (**206 passed**, 0.41-second suite
execution, no map bake), and locked release workspace/all-features build.
Eight new focused tests protect source/collision hydration, invalid controls,
relative-current density response, depth/view/drag/timestep behavior, impact/rise,
real ledge/ramp/ceiling/held-Jump behavior, strong sideways-current banks,
maximum authored rim reach and compiler key separation. Existing caustics
eligibility tests continue passing. Asset validation passes (620 assets,
0 warnings); texture/prop check results retain their existing art advisories.

The accepted campaign uses direct release candidate D (`f5d59e6f…`, preserved
as `source-binaries/places-native-candidate-d`), real gameplay delta, isolated
settings/state and one native process at a time. Final comment/source-location
corrections produced player `fb38b2b9…` with unchanged implementation; its exact
binary repeats the Pool loop/current-bank smoke and matched uncapped timing in
`pool-loop-release`, `flow-bank-release` and `perf-reviewed-after`.
Each case has a 40-second budget and exits 0 with correct package/member
identity, unchanged inputs, actual **1280×720** drawable (640×360 logical, 2×),
V-Sync off, Immediate presentation and zero missed ready surfaces. Settings:
High textures, High filtering, High lighting, Full lightmaps/reflections, bloom
on, 60° FOV; `flow-low-light-accepted` instead uses Low lighting/Static lightmaps
while retaining High texture/filter settings. Action/capture runs are correctness
evidence, not FPS comparisons. Explicit initial placements/item activation are
diagnostics; walking, jump, crouch, pickup and drop use production input paths.

| Directory | Observed result |
| --- | --- |
| `pool-loop-accepted` | Walk entry reaches feet −1.5, waist depth 2; crouch gives eye −0.7/depth 3. Rise reaches eye 0.511; west rim exit lands feet 0, grounded. Repeated jump entry settles to feet −1.5/depth 2; reset count remains 0. The capture named jump-entry is still airborne; its later settled capture proves entry. |
| `pool-bank-accepted` | First ascent reaches deck feet 0; descent returns feet −1.2/depth 2; second ascent reaches feet −0.017 near the top of the real ramp, grounded. No reset or water jump. |
| `pool-carry-accepted` | Normal E picks up the buoyant duck, carries it through waist water and up the bank to feet 0, then drops it on the deck with ownership cleared. |
| `flow-dive-accepted` | Looking down/forward while standing reaches eye −0.8, depth 3; held rise reaches eye 0.816/depth 1; releasing rise returns to depth 2. No collision recovery/reset. |
| `flow-bank-final` | At 0.8 m/s sideways current, feet follow the actual bank: −2.094 → −1.447 → −0.637 → 0. Grounded throughout; no water-jump activation, launch or reset. This fixes `flow-bank-r1`'s reproduced false ledge jump. |
| `flow-still-accepted` | Player X remains 5 with zero horizontal velocity. Duck remains X4 near waterline; basket sinks to Y−2.398. |
| `flow-east-accepted` | Player approaches +0.8 m/s (0.795 mid-run, 0.801 end); duck X12→13.894. Basket sinks and moves only X14→14.110 before bottom friction arrests it. Player is separated from props at Z8 versus Z6. |
| `flow-west-accepted` | Player approaches −1.6 m/s, then the real bank blocks it at X18.300; duck X20→18.052. Dense basket sinks. No boundary penetration/reset. |
| `flow-reversed-accepted` | Same centre-bay input/source except authored −X1.6: player reaches west boundary X10.300; duck X12→10.052. No geometry/lightmap rebake. |
| `flow-light-accepted`, `flow-low-light-accepted` | Turning sun focus off changes header count 11→0 and receiver energy to zero; restoring RGB [1,.15,.05] restores count 11 and receiver energy in that ratio. Motion clock continues while off. Matching static-camera 280×200 pixel regions are identical in the still bay but change 28,444/18,951 pixels in the moving bays. |
| `pool-main-view-after` | Matched render eye (8,1.6,7.9), yaw 0°, pitch −25°, compared with `pool-main-view-before`. Final surface and basin are visible without a broken rim or abrupt pattern seam. Both captures visually inspected. |

The delivered flow screenshot is `flow-overview/flow-bays.jpg` (original
`view.png` preserved), Library ID `libfile_64e1a1913b3c8191a9fb8403eea1dadc`.
The actual three-bay view was visually inspected and delivered to the main chat.
`flow-video/flow-bays.mp4` retains 24 native frames at 100 ms capture intervals
(2.4 s; 10 fps video cadence is not game throughput). Earlier Pool surface
screenshot `pool-surface-debug/surface-b.png` came from the corrected debug
candidate; the final release comparison is the separate view named above.

Failed exploratory attempts are preserved: the first native shader smoke exposed
the fixed 112/128-byte binding mismatch; a symlinked asset root was correctly
rejected by package containment; the first Pool preparation hit its timeout and
a sandboxed GPU preparation found no Metal adapter; native compilation then
completed successfully. The standalone Pool native launch cannot resolve its
hub transition without the full collection. Initial flow player/prop runs shared
Z6 and allowed player contact; the accepted comparisons isolate the player at Z8.
None of those attempts is presented as passing acceptance.

| Preserved artifact | SHA-256 |
| --- | --- |
| `source-binaries/places` (final reviewed) | `fb38b2b9731345593052d05d721b4a3248aa271ccc8871c73d589d600ec0204d` |
| `source-binaries/places-native-candidate-d` (accepted campaign) | `f5d59e6fcd276e3a44a2f114c87debafaedebed321f11a671a114bfc022338f8` |
| `source-binaries/places-compile` | `e663a7f0a7b361dbd5948ae418e037cd2852d120d602bbff5d80d8e8c8d8e2e5` |
| `The-Quiet-Collection-Stage3.placesmap` | `d6ee71343abbdd3a65c85eacc5ab54e8dd6944a5910eebd92ea12037712253ff` |
| `Pool-Stage3.placesmap` | `86236017987e9617b0dc02cf854c8a5f74b46ab94b485c1d5d36b663cbb293dc` |
| `Water-Flow-Stage3.placesmap` | `91a4b4a76f6b33d3f1e3501868102e9c1f03575aff6dad81b393afca5fd7121f` |
| `Water-Flow-Reversed-Stage3.placesmap` | `4dfef9e2c0df0bb889fc35d6940c8852455e73df063eeb012493eb2d22a35558` |

### Measured performance

Two interleaved pairs compare the preserved stage 2 player against water
release candidate D on the **same original collection package**, same fixed render eye
(8,1.6,7.9), yaw 0°, pitch −25°. Each measures 1,200 frames after 180 warmup
frames. No compiler, bake or other game overlaps these runs. Apple M2 Pro/Metal,
actual 1280×720 drawable, settings above. V-Sync is explicitly off; final present
mode is Immediate after initial surface setup. The loop has no active frame cap;
all measured surfaces present successfully. The display reports 120 Hz, while
measured throughput is 169–183 fps across the matched controls, median loop
times 3.1–4.2 ms, and mean swap only 0.067–0.077 ms. This verifies actual uncapped presentation rather than a
refresh plateau. GPU timestamps include scheduling stalls, not active utilization.

| Receipt | Mean loop ms | p95 loop ms | FPS | Mean GPU ms |
| --- | ---: | ---: | ---: | ---: |
| `perf-before-a` | 5.738 | 14.304 | 174.27 | 2.868 |
| `perf-after-a` | 5.775 | 14.854 | 173.16 | 2.915 |
| `perf-before-b` | 5.920 | 15.500 | 168.93 | 2.917 |
| `perf-after-b` | 5.893 | 14.614 | 169.69 | 2.965 |
| `perf-authored-after` (new collection) | 5.891 | 14.887 | 169.74 | 3.027 |
| `perf-reviewed-after` (exact final binary) | 5.883 | 15.417 | 169.99 | 3.146 |
| `perf-before-c` (fresh stage 2 control) | 5.587 | 14.959 | 178.98 | 2.781 |
| `perf-after-c` (fresh final pair) | 5.459 | 15.055 | 183.18 | 2.770 |

Equal-frame means of the first two pairs are 5.8289→5.8340 ms (**+0.09%**),
within observed run variation; no throughput improvement is claimed. GPU mean is
2.8928→2.9401 ms (+0.0472 ms / 1.63%) in those pairs. Checking the receipts
confirms unchanged base scene counts (32 draws, 20,216 triangles), no extra
render pass/mesh rebuild and no current in this control. Pairwise loop changes
are +0.64% and −0.45%.

The exact final binary's later smoke had GPU mean 3.146 ms, so a fresh bounded
baseline/final pair investigated that increase before handoff. It measured
5.587→5.459 ms loop (−2.30%) and 2.781→2.770 ms GPU (−0.36%): the higher GPU
sample did **not** reproduce as a persistent regression. This variation also
prevents attributing a fixed 1.63% GPU cost uniquely to surface motion. The
new authored collection control is 5.891 ms loop / 3.027 ms GPU, within the
observed ranges. These short runs establish no meaningful throughput regression
or optimization gain. High p95 tails exist in both versions and remain a stage 5
profiling target. This pass does not claim full-game performance or equivalence
at other resolutions/views.

Performance CSVs, per-run settings/environment, actual native identities and
`performance-comparison.json` preserve the evidence. No unbounded profiler trace
or archive was created.

### Limitations and next stages

Water is a planar low-poly presentation approximation; it does not deform the
silhouette or simulate fluid pressure/wakes. Adjacent sheets need matching motion
controls for a seamless continuous phase. The controller targets HL2 swimming
feel, with the explicit geometry/unit/current adaptations above; direct HL2
comparative input trajectories were not recorded. Land acceleration, air control,
bunnyhopping and crouch-jump parity remain out of scope.

The collection was repacked with bounded streaming compression; it is
467,503,143 bytes versus the original 449,857,977 bytes. Other map records are
unchanged, and this is not a storage improvement. Stage 4 should handle compiler/
storage work, including the broad executable-identity invalidation and repacking
cost. Full performance, editor polish, audio, UI/menu and visual styles remain
later queue stages. Cloud model integration is queued for a safe slot after this
handoff; no model assets were changed here. Final consolidated CI waiting/fixes
remain at the queue gate. No main merge, release or force push occurred.

## Cloud model integration — released native queue slot

This integration continues from water-stage commit
`5ebf0fab086cc527e9dfed290acd9301cbe1996a` on `0.13.0-staging`; local HEAD and the
live remote matched before work. The Mac queue had no competing game, compiler
or Cargo writer. All intake, checkpoints, old bytes, prepared packages, direct
binaries and native receipts are retained outside `target` in
`local/model-integration-0.13.0/`. The two pre-existing untracked Python audit
reports are untouched. Historical report consolidation and compiler/storage
optimization remain separate queue tasks.

### Exact runtime intake and asset contracts

Supported owned-Library materialization delivered the final runtime archives and
manifests to a separate intake directory. Archive CRC, traversal, duplicate-name,
link, encryption, bounded-size and path checks preceded adoption. Expected archive
byte counts/SHA-256 and every canonical file's manifest hash match. No private
URL was inferred, source archive regenerated, or authoring imagery substituted.
The unified `PLACES-144-VERIFIED-DELIVERY-INDEX.json` reconciles 243 unique runtime
files: **144 replacement GLBs and 99 PNGs**. The separate new Home door adds one
GLB and one PNG: **245 exact runtime files** in total. Full paths, byte counts and
SHA-256 values are retained in `adoption-candidate.json` in the local evidence
root; the frozen Library index is `libfile_4132eac1ec9c81918e2a8fe70d7ef599`.

| Cohort | Replacement GLBs | Runtime PNGs | Canonical repository locations |
| --- | ---: | ---: | --- |
| Outdoor houses/porches | 58 | 58 | `assets/environments/outdoor/props/` |
| Outdoor landscape | 28 | 28 | `assets/environments/outdoor/props/` |
| Frutiger Aero, 22 families | 24 | 6 | `assets/environments/frutiger_aero/props/` |
| Beach nature | 11 | 1 | `assets/environments/beach/props/`, `beach_nature_fitted.png` |
| Beach architecture/props | 18 | 1 | `assets/environments/beach/props/`, `beach_fitted.png` |
| Entity rigs | 5 | 5 | `assets/entities/{cockroach,sheet-ghost-cat,beach_seagull,beach_crab,beach_fish}/` |
| New Home folding door | 1 new | 1 | `assets/environments/home/props/closet_folding_door.{glb,png}` |

The catalog and accepted-export ledger pin actual geometry hashes, bounds,
triangle counts, texture edges and semantic slots. All 230 earlier accepted
ledger records remain equal. Six Landscape review-only modules, the ordinary
sheet ghost and Spoonerman remain untouched/excluded from this replacement
batch. Existing collision/placement/rig contracts and source masters remain
intact. The ghost-cat hem and Landscape MASK grass are intentional open surfaces;
reviewed compound contact seams were assessed using their specific notes rather
than a single watertightness heuristic. Office's 44 painted porch-post overrides
now cover the delivered trim slot as well as the original stock slot, retaining
the authored paint material. No global art, importer, draw or texture limit was
raised. Editable archives and review imagery remain outside runtime distribution.

The installed GLB set grows from 381 to 382 distinct paths. Decoded pixel
inventory is **87,797,760 → 91,926,528 bytes**, a **4,128,768-byte / 3.9375 MiB**
increase. Replacements account for 3,866,624 bytes / 3.6875 MiB; the new door adds
262,144 bytes. This is not zero overall texture growth. The production
`PropAssets` cache independently confirms the final total: 418 image references
share **187 actual decoded image allocations**, exactly 91,926,528 bytes. The
PNG-identity conservative checker reports 93,499,392 bytes / 89.168 MiB, below
the unchanged 106 MiB budget. These are installed decoded model-image totals,
not a claim that every image is simultaneously uploaded by a level, nor a GPU
allocation/utilization counter. Raw-image and native per-level receipts retain
those distinct scopes.

### Functional Home closet door

The actual Home showcase placement is `home_closet_folding_door`, position
**(0,0,8.6), yaw 90°**, at the bedroom's west wall, with a real backed alcove,
floor and ceiling. The continuous 300 mm west wall retains its existing passage and has a
1.96×2.18 m zero-sill rough opening for the full casing around the nominal
1.80×2.10 m aperture. This prevents frame stock/lighting anchors being embedded
in the wall and avoids the vertical seam from an earlier segmented wall.
The supplied final GLB remains SHA-256
`8e713092feff04f385606ad2b9e69bfdf07638ad5d3e44981cbb72894227e9d9`:
792 triangles, ten primitives, two opaque materials, one opaque 256² PNG and the
original LINEAR `fold_open` clip lasting 1.4 seconds.

Reusable `animated_door` support resolves named rigid nodes and samples that
same clip for both visual pose and live oriented collision. Four leaf cores,
two pulls and three fixed frame boxes leave the real aperture open; there is no
full-width solid prop box or single native door slab. Prospective motion probes
at most 5 ms of clip time, limits hitch consumption to 100 ms, stops at the last
safe pose and retries when clear. A new interaction reverses from that fraction.
Player, AI, fixed geometry, held/loose props and other leaf sets constrain motion.
The same live set feeds movement, interaction occlusion, item physics and
navigation clearance; posed bounds update culling. `animated-door-v1` makes the
new package contract explicit. Native slab doors and stage 2/3 safeguards retain
their interfaces. This supports upright rigid mechanisms, not general skinned
or triangle-mesh moving collision.

### Local gates and native acceptance

Strict workspace/all-target/all-feature Clippy passes (`clippy-reviewed2.log`).
The locked all-feature workspace release build passes (`build-reviewed-release.log`,
3m04s). All **210 Rust tests** pass in 0.40 s (`tests-reviewed.log`): the 206 prior
regressions plus three tiny delivered-rig pose/obstruction tests and one paused
held-body obstacle-pose contract. No full-map bake is part of ordinary Rust tests.
Asset validation reports 621 assets and zero warnings; prop budget validation
passes, while texture validation retains 96 pre-existing advisories.

Focused native fixture checks have exercised actual four-leaf opening,
mid-travel reversal, repeated traversal, 37° yawed placement, occupied close,
clear-close and loose-prop obstruction. A settled mug stops closing near progress
0.703 and clearing it permits the closed endpoint. During a held-mug close, the
existing carry controller moves the mug out of the approaching leaf path;
16 native captures record zero penetration. A suspected stale-query explanation
was disproved by matching live obstacle/body bounds, so no speculative physics
change was made. Ten delivered entity clips advance with original clip names;
`tail_tip` resolves and zero opacity suppresses its light. The full-lighting
checks below complete native acceptance. Diagnostic action/capture runs are not
performance comparisons.

Selective rebuilding already exists: `places-compile build <project> --level <id>`
reads the selected level (`src/map_compiler/project.rs:17`) and emits its own
package. The ordinary current-package check returns without rebuilding when
that member's inputs match (`src/map_compiler/mod.rs:349`); a separate prepared
lighting fingerprint can retain solved lighting for supported non-lighting edits.
Editing one level therefore does not inherently require rebaking every level.
Here, seven members actually use changed models. Pool and Arcade use none.

There is nevertheless over-broad catalog invalidation: `load_catalog_identity`
hashes the entire catalog, including unused entries
(`src/map_compiler/mod.rs:2058`), both package and prepared-stage keys include that
hash, and runtime validation requires its exact installed value
(`src/engine/package/world.rs:326`). This integration's **local, task-specific
script** `compose_collection.py` automates dependency-file SHA/size and used
catalog-entry equivalence checks, then rebinds only catalog provenance and its
manifest hashes for Pool/Arcade. Member selection is explicit. Their prepared
geometry, lighting, probes and original source/compiler provenance remain intact;
this is not a new supported automatic compiler reconciliation facility. Automatic
used-entry invalidation and the broad executable-identity key are recorded for
the **next storage/compiler optimization pass**, with no compiler optimization
implemented here. The revised later queue is engine/game performance, mannequin
ragdolls, then historical documentation cleanup.

Project distribution export also seeds each staged member from the previous
collection (`src/map_compiler/distribution.rs:178`, `project_cache.rs:9`) before
those checks. Thus a normal unchanged member can already be reused during a
collection export; it is the broad global identities above that made automatic
reuse miss for these otherwise unaffected members.

All seven changed members pass `verify --require-current`: Home, Hub, Office,
Winter, Beach, Aero and Lantern Hollow. The final nine-member collection passes
production validation with intact dependencies and zero warnings. Pool/Arcade
retain their original prepared payloads and tool provenance; their developer
currentness receipts explicitly remain false with `compiler executable changed`.
This limitation is preserved rather than representing retained work as a fresh
compile. The local rebinder preserves the engine's pretty manifest serialization,
so raw manifest hashes and actual native identity logs agree. Its first compact
JSON attempt passed content validation but failed the identity receipt gate;
that exploratory Pool receipt is not accepted evidence.

| Focused native evidence | Accepted result |
| --- | --- |
| `home-door-mounted-reviewed` | Actual Home, Full lighting. Closed walk stops at X 0.367; open entry reaches X −0.673, close reaches progress 0, interruption changes opening 0.426→closing 0.318, reopening reaches 1, exit reaches X 0.666. All 12 captures have reset_count=0. Ordinary E activates the real leaves/frame. |
| `door-fixture-r2`, `door-props-yaw-r1`, `door-loose-r1` | Player obstruction holds near 0.684; clear-close/reopen and repeated traversal pass. 37° yawed placement crosses normally; instances remain independent. Settled mug blocks near 0.703 and removal restores close. Earlier missed-pickup labels in props-yaw are not prop acceptance. |
| `door-carried-sweep-reviewed` | Actual E pickup and carry routing; all 16 captured penetration depths are 0. The carry system retracts the mug clear, rather than the closing leaf passing through it. |
| `entity-clips-reviewed`, `entity-close-full-reviewed` | Five replacement rigs, ten original clips advance; native sockets resolve. Full-lighting front/side views retain partial-opacity sorting and emission. Thirteen cycle captures span opacity 0.0158–0.9844 with the moving `tail_tip` attachment. Local node/skin/scene/clip JSON and animation/inverse-bind bytes match the originals across 67 joints. |
| `ghost-zero-reviewed` vs `ghost-absent-reviewed` | Identical room, lights and camera with the sole zero-opacity rig removed in the control: **0 changed pixels out of 921,600**, max channel difference 0. This directly checks absent depth, shadow and emission contribution. |
| `lantern_hollow-models-reviewed` | Four house facades, porch joins, pond landscape, ghost and campfire inspected. Fitted stock, aligned rail/post contacts, MASK grass, opaque masses and warm emission/light response remain coherent. Reviewed bargeboard/platform/core contact seams and inherited roof clearances remain intact. |
| `beach-models-reviewed`, `beach-town-final-reviewed` | Native pier, town/stairs/bunting, lighthouse, sea arch, grounded offset palm, cove/foam edge and three animal types inspected. Static OPAQUE surfaces, fitted materials, water-edge alignment and original collision surfaces/placement remain intact. Initial town camera was obstructed; the accepted town view uses the explicit fixed render eye(8,4,20). |
| `aero-models-reviewed` | Dome and partition glazing, fountain/bubbles, ceiling emission, upright fitted banners, reception and green/city backdrops inspected under Full lighting/reflections. |
| `office-models-reviewed`, `winter-models-reviewed`, `hub-models-reviewed` | Painted rail/post slots, actual upper ramp rails, winter landscape/boulder response, and all three changed Hub preview props inspected. An obstructed Office upper camera was superseded by the ramp capture. |
| `pool-water-final-reviewed`, `office-ramp-final-reviewed` | Pool walk/jump entry, submersion, view-directed dive, resurfacing and rim exit retain reset_count=0; ordinary upper-ramp ascent/descent reaches eye Y 8.8/5.2 with reset_count=0. These focus on preservation of accepted stage 2/3 behavior. |

Native captures were actually viewed for scale/orientation, texture fitting,
opacity/emission, lighting/shadows, joins and animation. Home's initial segmented
wall had a visible vertical join; a continuous wall with a clear-aperture-sized
cut then embedded the outer frame and darkened its spatial lighting. Cutting the
rough opening to the **outer casing dimensions** fixes both defects while
retaining the supplied model and ordinary lighting policy. Only Home was
re-prepared for these observed placement defects. The final Home overview,
close/open/inside/reversal captures retain the correct painted stock and visible
frame; no lighting boost, emissive override or artwork simplification was used.

Initial all-peer catalog rejection, missed prop-pickup setups, the too-short
480-second Office preparation, noncanonical rebound-manifest identity and the
Home placement prototypes remain local exploratory evidence. They are not
counted as passing acceptance. No extra full-map or quality matrix was repeated.

### Focused uncapped performance

Two interleaved baseline/candidate pairs per view measure 1,200 frames after 180
warmup frames, fixed render eye and 1/60 simulation delta. The preserved stage3
player/package/assets are the baseline; the reviewed player and new assets are
the candidate. No compiler or other game overlapped. Apple M2 Pro/Metal,
640×360 logical / **1280×720 actual drawable**, High textures/filtering/lighting,
Full lightmaps/reflections, bloom, 60° FOV. Every run records actual **Immediate**,
V-Sync off, zero missed ready surfaces, and no frame limiter. Throughput 184–623 fps,
median loop times below 8.33 ms, and mean swap 0.014–0.030 ms exclude a 120 Hz refresh
plateau. Capture/interaction runs supply no FPS claims.

| Fixed view, two equal-frame means | Before→after mean loop ms | Before→after FPS | Loop change | Base scene draws / triangles before→after |
| --- | --- | --- | ---: | --- |
| Lantern house front, eye(9,1.6,11), yaw 0 | 5.2349→5.3559 | 191.03→186.71 | +2.31% | 409 /1,367,966→419 /1,379,130 |
| Beach pier, eye(10,1.6,−3), yaw 0 | 1.6114→1.6635 | 620.57→601.16 | +3.23% | 22 /3,069→27 /3,605 |
| Aero atrium, eye(2.5,1.6,10.1), yaw 38 | 1.6564→1.6512 | 603.73→605.62 | −0.31% | 85 /20,378→102 /23,734 |

Pairwise loop changes are Lantern +3.30/+1.32%, Beach +2.47/+4.00%,
Aero +1.20/−1.82%. GPU means are respectively 5.300→5.377 ms,
1.576→1.430 ms and 1.118→1.185 ms; timestamps include scheduling stalls, not
utilization. These short controls show modest measured costs with the additional
art/draws in two views and variation around equal throughput in Aero. They do
not establish a performance gain, confidence interval, thermal-controlled result
or full-game performance guarantee. Raw CSV/settings/native identities and
`performance-comparison.json` preserve every run. The later Home-only rough-cut
refinement leaves all three timed member records **byte-identical**, so no FPS
repeat or unrelated optimization was performed.

### Artifacts, limits and handoff

| Preserved final artifact | SHA-256 |
| --- | --- |
| `source-binaries/places` | `644eb4f92641105c7cdc21c12723157e7c8d714c46a919459f5055ece51d8076` |
| `source-binaries/places-compile` | `d5e37d74e3c35a523126d142efcab1d2e202821906fed55c52df977026a23e9f` |
| `The-Quiet-Collection-Models-Accepted.placesmap` | `1a4ecdf4ae0f87d0403c9477e090288373557b8e8b5d044e6894bc6afc6c9f73` |

The accepted collection is **472,639,184 bytes**, versus 467,503,143 at stage3;
this is not a storage improvement. The final package is installed as
`assets/levels/environments.placesmap`; old packages, model/catalog bytes and
all receipts remain available locally. Home/Hub/Office/Winter/Beach/Aero/Lantern
preparations took approximately 65/48/1,200/474/252/245/1,239 seconds before the
small Home fit iterations. Lantern's roughly eleven minutes of Static preparation on one CPU core
and subsequent transport progress with six workers are retained for the next compiler
pass; no duplicate process was launched during connection fluctuations.

Acceptance is scoped to this Mac/Metal, the measured resolution/settings and
representative placements. The mechanism is upright rigid-node OBB collision,
not general skinned/mesh collision, native slab lock parity or a full-game
platform campaign. Two legacy geometry-check service-passage warnings and 96
texture advisories remain unchanged. No global limits were raised and no art
bytes simplified. The next work is **storage/compiler optimization**, then
engine/game performance, mannequin ragdolls and historical docs cleanup. Those
passes are not started here. All work remains on staging; no main merge,
release, force push, CI wait or `cargo clean` was performed.

## Map storage and compiler optimization

This stage starts from integration commit `655a6d8433a1ac24fbafe4e9fd6fa15d1d7cc3ad`.
It preserves the accepted art, geometry, collision, animation, every supported
lighting mode and circuit state. Evidence is under
`local/map-storage-0.13.0/final/`; the original collection, compiler/player,
rejected storage prototypes and earlier receipts remain outside `target`.

### Implemented behavior

New compiler provenance revision 2 records a validated, transitive catalog
closure for each level and preparation-source identity. Referenced materials,
textures/masks, fixtures, placed/template models, decals and missing lookups
participate. File dependencies still require their size and SHA-256. An unused
catalog entry or unrelated CLI/game/editor source change does not invalidate
lighting; shared preparation/engine source changes conservatively do. Compiler
identity includes target/features, dependency lockfile, fixed preparation assets
and the compiled-in model registry. It deliberately remains conservative within
shared engine code, including comments and renderer changes.

`verify --level ID --json` and `map_compiler::verify_project` expose per-level
`current`, `refresh`, `bake` or `invalid` status, `bake_required`, integrity and
named reasons for the later editor pass. Metadata/navigation refresh retains
prepared lighting. A complete unchanged export verifies source bytes, source
manifest/index agreement and every member's payload hashes, then leaves the
archive untouched. Partial export directly streams verified current members;
only affected members enter the existing seed/build pipeline.

Static directional visibility now uses bounded conservative trees for walls,
horizontal solids and oriented props, retaining the original exact leaf tests.
Unsupported/nonfinite or oversized geometry falls back to the linear oracle.
The derived trees are absent from wire data and bounded below 24 MiB combined.
Immutable visibility/index allocations are shared between Static switch-state
clones; state-dependent light values and baked colors remain separate. Vertex
preparation now observes the existing cancellation token inside long batches.

Static (`off`) still computes room/fixture lighting, hard-shadow visibility and
vertex colors from local light, five sky rays and authored directional lights;
it also prepares authored circuit-off vertex colors. It means no lightmap
atlas, not no lighting (`render/common/api.rs:762`, `lighting/bake.rs:1939`,
`map_compiler/mod.rs:1720`, all under `src/engine/` except the last).
Full adds chart planning, direct/two-bounce transport, switchable-light data,
irradiance probes, HDR atlases and reflection captures. The reported forced
Static command includes navigation, records and compression, not Full transport.
Static is a separate playable lighting variant; the default export builds both
Static and Full, rather than treating this timing as the entire Full solve.

The explicit `optimize-storage INPUT --out NEW.placesmap` operation validates
the existing package/dependencies and performs no lighting or geometry bake.
It chooses established PPS1 revision 2 or exact byte-lane shuffled revision 4
by measured compressed cost, with bounded scratch and unchanged decode limits.
Readers retain revisions 1–4; new required capabilities make older players
refuse unsupported packages explicitly. Archive format remains version 1.
Archival migration uses Deflate-9; normal builds retain Deflate-6. No geometry,
float, texture, lightmap or probe precision is reduced. Source/provenance and
all non-prop payload bytes survive unchanged. Input/output guards, atomic
publication and owned temporary cleanup preserve the last usable package.

### Measured storage and compilation

The current accepted nine-level collection was remeasured, rather than using
the historical 449.9 MB baseline. Actual ZIP payload sets count shared records
once; per-map rows overlap and must not be summed.

| Collection measure | Before | After |
| --- | ---: | ---: |
| Complete archive bytes | 472,639,184 | **446,058,430** |
| Unique uncompressed ZIP record bytes | 1,499,883,500 | 1,467,307,398 |
| Physical ZIP records | 207 | 205 |
| Unique reconstructed PMP1 prop bytes (21 records) | 1,009,761,466 | 1,009,761,466 |
| Static-only payload bytes compressed | 52,854,244 | 55,249,011 |
| Full-only payload bytes compressed | 269,170,699 | 255,836,420 |
| Static/Full shared payload bytes compressed | 149,073,748 | 133,645,923 |

Savings are **26,580,754 bytes (5.62%)** overall and 32,576,102 uncompressed ZIP
record bytes; reconstructed prop sizes remain unchanged. Static-only cost increases by 2.39 MB as the chosen factoring
changes shared-record boundaries; the collection total includes this cost.
Pure Deflate-9 of the original representation yields 448,866,575 bytes;
adaptive structural storage therefore saves another **2,808,145 bytes** beyond
that compression-only control. This is not a 26.6 MB structural-only gain.

| Map, both modes, unique referenced compressed payload bytes | Before | After |
| --- | ---: | ---: |
| Hub | 5,610,257 | 5,295,731 |
| Home | 12,406,882 | 11,689,561 |
| Pool | 21,068,800 | 19,692,897 |
| Office | 44,476,313 | 41,027,309 |
| Arcade | 93,796,206 | 87,600,005 |
| Lantern Hollow | 185,598,040 | 176,779,884 |
| Winter | 72,771,362 | 69,369,323 |
| Beach | 19,222,979 | 17,926,294 |
| Aero | 16,452,819 | 15,558,619 |

The per-map figures are actual referenced ZIP payloads, excluding archive
headers/index and external assets, rather than hypothetical independent
archives. `storage-before.json` and `final/storage-after.json` also retain each
map's Static/Full/shared and uncompressed breakdown. Standalone real Static
builds are 76,163,556→76,171,479 bytes for Lantern and 549,574→554,033 for Home;
their small provenance growth is not a storage win. Unconditional shuffled
storage grew Lantern Static substantially in a rejected prototype; adaptive
selection retains its established representation.

Lantern Static forced compilation fell from **700.919 s to 23.115/22.853 s**
in two final-binary repeats, with identical reconstructed props and every other
prepared record. Earlier candidate repeats were 22.439/22.289 s and remain
preserved. Prop geometry/color preparation fell from 680.321 s to
7.469/7.398 s; architecture from 9.810 s to 0.688/0.680 s. The baseline was one
historical controlled invocation, with release linking overlapping part of it;
these are observed timings, not a statistical confidence interval. Candidate
repeats run serially, record host load/CPU/RSS, and emit identical archives.
The final warm integrity-checked no-op is 0.545 s. Maximum final repeat compile
RSS is 1,365,999,616 bytes; a complete baseline peak was not captured.

The dominant hotspot was Static vertex lighting in prop preparation (97% of
baseline wall time), across about 5.21 million vertex slots. Prop coloring calls
`global_surface_light` (`src/engine/render/common/props.rs:274,924`), including
the unchanged five sky rays in `src/engine/lighting/bake.rs:1965` and authored
directional lights. Previously each visibility ray scanned all walls,
floor/ceiling solids and props. `Visibility::occludes_direction`
(`src/engine/lighting/visibility.rs:2592`) now rejects entire groups that the ray
cannot intersect through `src/engine/lighting/directional_index.rs:106`, then
runs the same exact leaf tests. This skips irrelevant intersection work without
changing samples or geometry. Forced builds bypass reuse, so the large cold
gain is not a cache hit; Full transport is unchanged. Architecture also saves
about nine seconds, while trying both lossless storage candidates adds work.

Home Static remains approximately 0.546→0.542 s, and the tiny Static/Full
switch fixture 2.180→2.189 s: neither establishes a speed gain. Home switch
clones share 158,200 visibility/index bytes, clone in 0.008 ms, and prepare the
alternate state in 20.888 ms. The tiny two-level no-op takes about 0.11 s in
both tools, but the new tool reports `rebuilt=false` and performs no publication.
Costly Full light transport and the remaining prop factoring/compression time
are retained findings for later work, rather than another full collection bake.

A user-requested bounded second outdoor comparison uses Beach, identical source,
catalog/existing dependency hashes, 12-worker budget and forced Static-only requests:
**57.048→2.472 s (23.1×)**, with exact props and every other prepared payload.
Its architecture phase falls **49.720→0.469 s**, props **5.515→0.096 s**; the
same ray acceleration therefore helps architecture as well as model coloring.
Warm reuse takes 0.223 s. Peak RSS is 77,299,712→82,919,424 bytes, and standalone
archive size 1,210,482→1,214,207 bytes (provenance overhead, not a size win).
Together with Home's unchanged small indoor case, these single-pair controls
demonstrate gains on two expensive outdoor cases, not a universal speed factor
or a Full-bake measurement. `campaign-beach.json` and `beach-inputs-exact.json`
retain the measurements and equivalence checks; no full collection bake ran.
The new compiler also records Beach's unchanged `collection_hub.png` decal,
which the old dependency collector omitted; this is tracking, not an asset edit.

Archival migration takes 100.821 s (100.919 s process wall), with measured peak
RSS 2,079,588,352 bytes and observed temporary disk peak 1,913,365,828 bytes
(250 ms sampling). A final-binary cancellation/restart repeat takes 102.440 s
and emits the identical archive SHA-256. It processes one member's prop streams at a time and
disk-spools unique project records under existing caps. These are measured
campaign costs, not an assertion that whole-process RSS equals the decoder's
1 GiB charged working-set limit. The 65,536-vertex shuffle scratch is bounded
to 2,949,120 bytes and included in that limit; no hostile-input cap was raised.

### Verification and native preservation

All nine member prop reconstructions, every non-prop prepared payload and all
12 recovered source files are byte-identical to the accepted input. Complete
package validation passes with 375 unique installed dependency identities.
Repeated Lantern builds are archive-identical. Focused reuse checks cover:

- Unchanged export and unrelated catalog/asset/code edits leave the archive unchanged;
  runtime validation also accepts unrelated catalog metadata changes.
- One level's metadata refresh retains lighting; a light edit rebakes that
  level while its peer directly reuses verified records.
- A shared material edit invalidates both dependents; changed shared model
  bytes fail `--require-current` for both. Corrupt payloads are refused.
- Failed source admission preserves the previous output. Observed in-flight
  cancellation exits 130, preserves it, removes owned staging, and restarts
  successfully. The storage operation also cancels/restarts with unchanged input
  and identical completed output; incomplete standalone transition packages are
  refused. Source-manifest/index disagreement cannot return a no-op hit.
- An older compiler explicitly refuses the new storage capability. Legacy
  revision 1 provenance remains runtime-valid under its original checks;
  acquiring revision 2 reuse keys requires one real rebuild. Storage migration
  intentionally preserves old provenance and does not claim all nine are current.

Native Mac/Metal checks use the actual selected package/member identities,
**V-Sync off, Immediate presentation, 1280×720 drawable**, High textures/filter,
Full reflections and bloom. Package comparisons hold the stage player constant
to isolate storage; derived index storage is charged separately by its retained
byte accounting. Matched Home/Lantern captures and all six
Static/Full on/off/restored captures have **zero changed channels**. Native
Home folding-door traversal/reversal retains nine collision boxes and zero
resets; Pool entry, underwater dive, resurfacing, rim exit and jump entry pass.
No runtime source preparation occurs.

Two alternating cold-application/warm-OS-cache Hub→Home→Pool→Hub runs give
per-run load-window medians 390.7/379.4 ms before and 394.3/385.3 ms after;
maxima stay approximately 3.23–3.27 s. Short Home initial loads are
819.6→828.3 ms, Lantern 5,147.8→4,992.2 ms. These do not establish a load-time
gain or material regression. Retained prepared CPU bytes are unchanged:
Home 84,680,421 and Lantern 965,299,278. Lantern's observed native peak RSS
increases **1,915,797,504→2,243,395,584 bytes** in the short capture pair;
Home stays approximately 499 MB. The peak increase is a limitation for the
performance pass, not a resident-memory saving. At world commit, RSS also
remained higher: **1,896,677,376→2,227,077,120 bytes**. These short runs do not
establish the increase's lifetime or cause. GPU allocations and RSS are not
added to the decoded-record totals.

Strict workspace/all-target/all-feature Clippy, workspace tests (**212 passed**),
format, diff checks and locked all-feature workspace release build pass. Added
tests are small exact-oracle/catalog/tile-boundary regressions; none bake a
shipped map. No full-map CI workload was added. Initial harness runs selecting
the installed package or redundantly toggling a shared circuit were caught,
excluded and retained; corrected package-identity/state runs pass.

The validated smaller collection is installed at
`assets/levels/environments.placesmap`, SHA-256
`8012dbb71ba73fbc85d81582511e0d99a4e7a2103d5dfb48fbce1a33e0cf0783`.
The original `1a4ecdf4…` collection and prior stage3 assets remain preserved.
Acceptance is representative Mac/Metal coverage, not every map/platform/view
or a fresh nine-level Full bake. Capture/action diagnostics make no FPS gain
claim. Runtime tracing by source shows a conditional frame path through moved
props/characters' center-light refresh (`wgpu/renderer.rs:1317,2458` →
`common/light_transport.rs:710` → `fallback_entity_texel` → `occludes_direction`),
when no valid irradiance sample resolves. Exact placement/lighting keys skip
unchanged receivers; valid Full probe samples avoid this fallback. Directional
caustics receiver setup also uses it (`wgpu/caustics_field.rs:380,517`), then
caches its factors. These are runtime profiling leads, not established FPS
gains; steady static geometry uses prepared colors/atlases. Paths are under
`src/engine/render/`, with fallback in `src/engine/lighting/bake.rs:2007`.
The next stage is engine/game performance, then the separately authorized
additional engine optimization sweep, mannequin ragdolls, docs cleanup, editor,
audio, menu/UI, styles and final validation.
No main merge, release, force push, CI wait or `cargo clean` occurred.

## First engine/game performance pass — 2026-10-21

This pass implements measured CPU fixes and explicit inactive-world teardown for
[#27](https://github.com/csd113/Places/issues/27) and
[#28](https://github.com/csd113/Places/issues/28). It does not close either issue
or replace the separately authorized fresh engine sweep. Baseline is
`f38c638e7a7ba6fa93136636600b82e0ae4f8937`; the initial checkpoint player SHA-256 is
`b0c07d737012ced1c6cbdf4b2a81cb506e664453a8aa21ce8ef5167c8755b216`.
Checkpoint `2c9671b2a850deb7585f082a522c3da8824f4329` binds that binary's source;
the expanded final binary and results appear below. Baseline and
final executables, the 446,058,430-byte collection, lab packages, inputs and raw
clips are preserved outside `target` under `local/performance-0.13.0/`.
The collection's SHA-256 remains `8012dbb7…0cf0783`; no collection rebake ran.

### Implemented causes and preserved behavior

- Moving-caster revisions caused stationary receivers to repeat their static
  probe/transport/room-light ray work. Each receiver now retains up to 2,048
  exact directed endpoint/type/label answers. Model/bounds, field/scene,
  lighting-state or resource-revision changes invalidate them. Every movable
  transmittance query still runs; overflow computes normally. No shadow taps,
  probes, sources, visibility support or quality are removed. This is separate
  from the previous compiler pass's directional index; no isolated runtime FPS
  claim is made for that older change.
- Rigidly weighted skin normals reuse each joint's inverse transpose for its
  current pose. Only exact `[1,0,0,0]` weights take this path. Blended weights
  retain the previous blended-matrix inverse, including singular behavior.
  Posed positions and whole-body visibility/bounds are unchanged.
- Sequential AI work now loans one world-owned navigation scratch allocation to
  each agent, preserving query order, replanning/visibility budgets and results.
  The former three cell arrays could retain about 24 MiB per agent at the
  2,097,152-cell limit, or about 3 GiB at 128 agents, before heap storage.
  This removes that multiplication; it is not a claimed path-search speedup.
- Missing GPU textures prepare/upload in eight-sheet chunks instead of staging
  every miss together. Eight 1024-square RGBA mip chains require about 43 MiB
  of prepared pixels; decoding/image caches and driver allocations are separate.
  Upload order, duplicate plans, pixels, filtering and worker ceiling are kept.
- Explicit Main Menu entry cancels CPU/GPU preparation, drops pending commits
  and world audio, scripts/AI, physics/carry, map geometry/materials/textures,
  atlas/probes, effects and map-owning GPU bindings. Worker-owned builds/images/
  models retire serially. Two cancellation orderings have deterministic small
  regressions, including maintenance replaced and then canceled after a running
  job's last release check. Pause retains its world. Shared device/surface,
  fonts/UI, drawable targets and the fixed flashlight target remain. The old
  whole-map menu backdrop is intentionally gone; the later independent menu
  scene is outside this pass. Maintenance owns no loading status UI.
- Prop-batch readers and writers now agree on 65,536 addressable vertices;
  65,537 is refused before vertex allocation. No cap was raised, and validation
  of the existing full collection passes.

### Benchmark and measurement contract

`projects/performance-lab` is a 16×12 m ordinary editor project: fifteen
bookshelves/forty-five book stacks; twenty-four dynamic cups and a pickup
cushion with bounded activation/reset; twelve animated wandering mannequins;
four room fixtures, fog and normal flashlight support; and explicit Luau
per-frame actor queries plus enter/exit physics and three-second, one-active
visitor spawn handlers. It supplements the real maps. It adds no runtime-only
map construct, new visual asset, raised authoring limit or CI map bake.
The existing editor's Save+Build, close/reopen and complete Project Export
succeeded. A final-editor copy was also nudged 0.25 m, saved, closed, reopened,
restored and exported through visible controls. Its only serialization
normalization is explicit `rooms[0].floor_y=0`; all other authored values are
preserved, and all fifteen non-project exported members are identical.
`lab-editor/`, `lab-export/`, `editor-roundtrip-receipt.json` retain the evidence.
The baseline level package is 7,686,490 bytes; both lighting variants build in
about two minutes. Source/usage and repeatable inputs are in the project's
README and `tools/bench/performance/`.

All accepted FPS runs use the same M2 Pro 12/19-core, 16 GiB unified-memory
Mac14,9, macOS 27.0.1/26A434, Metal, Rust 1.99.1 release profile
(opt-level 3/LTO/one codegen unit). High textures/filtering/lighting, Full
lightmaps/reflections, bloom and 60° FOV are held. Logs verify actual Immediate,
V-Sync off, no frame limiter and zero missed ready surfaces. Throughput is not
a refresh-rate plateau. Drawable sizes below are physical pixels: logical
640×360 yields 1280×720; logical 1280×720 yields 2560×1440.

The final lab ABBA set has 2,400 samples after 120 warmup frames per run,
matched fixed 1/60 simulation work. Real interaction sets last twelve ready
wall-clock seconds with normal delta; matching phases are selected from native
ready/present timestamps, excluding startup. Recovery is 6–11.8 s, held work
1.4–3.2 s, closet near 2.4–4 s, facing away 4.4–5.3 s and retreat 7–11.8 s.
A fixed frame count with wall-clock actions was rejected for interaction claims.
GPU timestamps are asynchronous pass spans including scheduling stalls, not
utilization or a quantity to add to CPU time. Capture runs supply no timings.
There are 26 primary comparison runs and 57 completed timing-related receipts
including exploratory/diagnostic runs; these are not 57 independent replications.

| Case / drawable | FPS before → final | Mean CPU update ms | Loop p99 ms before → final | Interpretation |
| --- | --- | --- | --- | --- |
| Lab / 1280×720, two runs/build | 377.22–381.73 → 412.36–412.63 | 2.00 → 1.80 | 3.51–5.97 → 3.57–4.71 | About 8.7% throughput and 10% CPU savings; tail ranges overlap. |
| Home books recovery / 1280×720, two runs/build | 513.5–539.9 → 627.6–781.7 | 1.23–1.36 → 0.55–0.65 | 2.62–2.68 → 2.15–2.80 | About 54% less CPU update work; pacing varies. |
| Arcade bowling post-drop / 1280×720, two runs/build | 94.9–115.9 → 135.0–149.5 | 7.32–9.27 → 5.20–6.11 | 13.53–13.59 → 9.02–9.48 | About 32% less CPU work; substantial residual cost remains. |
| Home cushion post-drop / 1280×720, one pair | 573.0 → 643.4 | 1.11 → 0.50 | 2.56 → 2.60 | A CPU saving, not proof of resolving the reported persistent collapse. |
| Pool closet near / 1280×720, one final pair | 627.4 → 375.0 | 0.196 → 0.234 | 3.04 → 17.52 | Regressed pacing run; cause unresolved, no gain claimed. Other repeats span 525–776 FPS. |
| Busy Arcade / 2560×1440 | about 244 → 244.62 | 1.16 → 1.05 | 6.47 → 6.14 | Effectively unchanged throughput; GPU mean 4.44 ms and 308 scene draws remain. |
| Arcade bathroom / 2560×1440, one pair | 366.64 → 364.90 | 1.14 → 1.02 | 6.60 → 6.84 | No reliable throughput gain; GPU mean 4.13 → 4.12 ms. |

An initial lab pair of 176→413 FPS was superseded by alternating repeats;
it is not the accepted gain. Pool's final near-phase GPU mean is 0.958→0.992 ms
and p99 1.27→1.74 ms while the loop tail grows much more. Its broader scheduling/
submission regression is a lead, not explained away as a physics fix. Quiet Retina Home is also unchanged (512.6→517.7 FPS, CPU 0.107→0.109 ms). Busy
Arcade GPU p99 is 4.59→4.51 ms; bathroom is 4.97→4.97 ms. Books' sleeping
physics takes about 0.007–0.008 ms/frame. Bowling recovery solver means are
0.043–0.047→about 0.042 ms, with zero to 0.202 mean awake bodies across runs.
Thus neither CPU saving is presented as a solver optimization. Real-time
trajectories/drop positions vary slightly; the workload and input contract
match, rather than identical physics states at every rendered frame.

### Actual repros, scripts and visual correctness

Home cushion idle→aim/pickup→moving carry→drop→sleep/recovery passes, with no
player reset. Books placed on the bed, ordinary jump/landing and retreat pass;
two bodies settle, an item-pair contact is observed, and the player recovers to
the floor. The original severe persistent Home slowdowns remain unconfirmed
and unresolved; stage 1 non-reproduction is not a fix. Pool approach/near/facing/
retreat runs cover the closet at the recorded distances; the original severe
case is not resolved. Arcade's busy/bathroom views and bowling held/contact
case are measured separately above. The bowling residual and Pool pacing remain
fresh-sweep leads. Final-binary folding-door blocking/open/traverse/close/reverse
and Pool entry/dive/resurface/rim-exit/jump-entry checks pass, preserving stage 2
collision/controller/prop and water behavior.

The lab activates all 25 bodies, then returns all to dormant through its normal
Luau reset. Observed peak solver-frame time is 0.165 ms. Visitor count is
12→13→12→13 on spawn, expiry and reentry. Twelve host actor queries per update
cost 0.078 ms mean, 0.105 ms p95 and 0.134 ms maximum across 64 diagnostic
invocations, including native capability setup. A separate early visitor
invocation costs 0.034 ms with one native call; creation is visible by the
0.8-second capture and expired by four seconds. This is an input-to-observation
bound including movement, not an isolated spawn-only speedup. Scripts were not
rewritten to manufacture an FPS gain. A tiny normal project deliberately
requests a 3 MiB Luau string under the existing 2 MiB VM cap: it fails closed,
discards staged actions, remains playable, repeats after reload at the same
467,256 VM bytes and releases to zero at menu. General host/driver OOM recovery
is not demonstrated by that controlled VM failure.

Matched frame-300 Home, Pool, Arcade and lab captures have **zero changed RGB
channels**. Native moving/carry and animated-doorway views remain intact; no
culling, source state, simulation accuracy or visual quality was reduced.
The native editor round trip, all captured frame paths and the accepted input
packages remain preserved, including failed/excluded harness attempts.

### Inactive maps, active work and memory

The heavy cycle is Lantern→Hub→Home→Lantern→Hub→Arcade→Lantern→Hub→menu.
Every retired Lantern prepared-record/build weak probe reaches zero after
leaving it. Hub/Home may retain passive prepared records in the existing bounded
LRU. The final menu has zero active prepared bytes, scripts/VM bytes, AI,
physics bodies, pending commit and map texture-cache entries; every retired
probe is zero. Canceled Lantern cannot resurrect after menu; Home reload and a
second menu release pass. Pause retains its world until explicit menu entry. A direct Lantern pause→resume→menu check additionally preserves all 965,299,278 prepared bytes during pause/resume, then releases its final record/build owners to zero; its menu cleanup callback takes 52.462 ms in one diagnostic event (`heavy-menu-results.json`).

A final 319.3-second campaign completes 42 requests/commits, 28 graphics
transitions, 29 drawable observations and seven menu returns while repeatedly
activating/resetting physics and spawning/expiring visitors. Every menu has
zero map ownership. Live menu GPU counts stay bounded at 16 textures and 23
bind groups, with 29–35 buffers and 27–31 views depending on fixed/drawable and
in-flight state. Prepared-cache pressure through Hub→Home→Beach→Aero→Hub
releases evicted Hub/Home/Beach records; menu releases the remainder.
Lantern active RSS snapshots are 2046.9/1922.2/1862.6 MiB; the heavy-cycle final
menu is 794 MiB. Extended menu RSS varies 603.9–841.6 MiB and later falls again.
RSS alone does not measure live old-map ownership: passive bounded caches and
allocator/driver capacity are distinct. CPU weak ownership and live GPU objects
are checked independently. Metal HAL byte counters are unavailable;
engine texel accounting is partial, with sharing, and is never added to RSS as
if M2 RAM and VRAM were independent pools.

The user's prior-map-CPU hypothesis was tested with exactly the same Hub
position (12,1.6,7), view (0,0,−1), settings and 39 scene draws. Fresh process
2–10 s measures **582.6 FPS / 0.167 ms CPU update**. After one and two heavy
cycles the same windows measure **625.2/635.9 FPS / 0.132/0.149 ms**, loop p99
3.05→2.98/2.91 ms. Old script modules, AI agents and physics work are absent;
the single prepare worker is idle after the completed result, with no pending
commit. RSS at Hub commits is 354.5→1130.5→1617.9 MiB. Passive cache/capacity
retention therefore does not establish active ticking, and no transition
slowdown is supported in this tested case. This does not rule out memory-pressure
or other-view effects generally. Menu retention was a real ownership defect;
it has not been established as the cause of gameplay FPS drops.

A separate no-capture heavy-transition run records a kernel native peak RSS of **2,679,783,424 bytes (2.50 GiB)**. Fresh Home/lab/Arcade run peaks are about 488/466–483/1,440 MB. One serial run's request-to-commit latencies are Lantern 4.80/4.72 s, Hub 0.405/0.298 s, Home 0.677 s and Arcade 8.21 s; these are observed operation costs, not a matched speedup claim. `transition-peak-results.json` retains exact identities, peaks and timings.

The earlier approximately 312 MiB loading-peak increase remains a documented
lead, not a proven transient or a solved resident-memory regression. The
historical world-commit RSS increase is retained above. Lower active-map RAM
was not pursued at the cost of speed or quality.

### Resource-limit audit and limits of acceptance

`local/performance-0.13.0/resource-audit.json` retains the full dated inventory:
units, source/enforcement, producer/reader agreement, rationale, failure behavior
and unbounded paths. MiB means 1,048,576 bytes. Counts/bytes/overflow, archives,
temporary staging and in-flight ownership were inspected; caps are admission
ceilings, not a promise that all maxima fit simultaneously.

| Domain / source owner | Limits and failure contract audited |
| --- | --- |
| Package/archive readers and writers | Ordinary/project transport 1/2 GiB plus framing; 8,192 entries/8 MiB directory; validated project allowance; decoded aggregate 1/2 GiB; default member 256 MiB, mesh/props 512 MiB; smaller manifest/material/chart/lighting/collision/nav limits. Bounded decompression, checked counts/offsets, exact identity/hash and source recovery remain. |
| Lossless prop storage | 16 MiB segments, 64 MiB recipe, 512 MiB reconstructed record, 1 GiB charged working set, bounded shuffle scratch; 65,536 batch vertices now enforced by writer and reader. Segment staging plus reconstructed output remains a peak lead. |
| Authoring and models | 32 MiB level JSON, 24M estimated vertices, numeric extent/area/height and every per-feature count; GLB 64 MiB, depth/visits, 65,535 vertices, primitive/material/image/joint/clip/channel/morph limits and reviewed triangle exceptions; 4,096 static model identities/24M transformed vertices. Refusal/fallback preserves prior valid state. |
| CPU image/material/prepared ownership | Decoded material table 1 GiB; PNG 32 MiB encoded and class-specific dimensions; prefetch 64 images/four workers; optional image cache 256 entries/256 MiB; prepared LRU three entries/192 MiB, oversized worlds uncached. One running/newest-pending preparation and bounded channels; menu serially clears caches. Prefetch and old/new install overlap are distinct from retained accounting. |
| GPU images, atlases, uploads and frames | Optional texture retention 256 entries/256 MiB at level boundaries; eight-sheet staging and at most eight workers; one pending install, nominal 4 ms slice, two in-flight frames, device/drawable limits. KTX dimensions/mips/layers/512 MiB; atlas 384 MiB plus framing, page/switch caps; reflection encoded/runtime probe limits. One large upload can exceed its slice. |
| Lighting/visibility/fog/effects | Field 262,144 probes/256 MiB/axis 64; direct/switch/blacklight/dynamic slots; 4,194,304 transport triangles, BVH/bounce/worker and spatial-grid bounds; new 2,048 fixed-size exact answers per receiver, lazily allocated and world-owned; fog/void/effect/particle/snow/shelter limits. Overflow never changes a cached ray's answer. |
| Dynamic rendering and physics | 256 render objects, aggregate vertex/index admission, 128 characters; 128 physics bodies and 1,024 explicit fixed-prop placements (not all world collision shapes); 12×1/120 steps and 0.1 s delta bound; compound/hull/extent/position/recovery/impact caps. The 256 item-contact diagnostics and 1,024 onset history do not truncate Rapier solver contacts. |
| AI/navigation | 2,097,152 cells/columns, class/portal/ID and 128 MiB nav limits, A* expansion cap, eight queries/tick, stimulus bound; one shared world scratch now. Failure/defer and query ordering remain unchanged. |
| Entities/spawns/sequences/Luau | Binding/action/queue/drain/depth/timer/volume/item/path limits; templates/points/groups, 512 live spawns and 64 requests/tick subject to lower downstream caps; sequence count/steps/active/duration caps. Luau 16×64 KiB sources, 2 MiB VM/module, 64 calls/128 actions per frame, 256 native calls/32 actions per invocation, time/instruction/diagnostic caps. Staged failure/reset contracts remain. |
| Audio/project/editor/diagnostics | Audio file/decode/voice/callback bounds; project manifest/256 levels/scripts/recovered source/thumbnail limits; editor history/serialized bytes, one oversized revision, script buffers and bounded job feedback; console/input/history/output, acceptance scripts, GPU timing slots, log and developer-object limits. Persistent installed packages/saves/editor outputs are not silently evicted. |

Boundary coverage includes archive framing/decompression/aggregate validation,
material admission before later decodes, dynamic overflow preservation, Luau
host/frame budgets and the new four-byte prop boundary regression. Native
oversized-cache bypass/eviction, canceled jobs, settings/resize retirement and
controlled VM allocation failure are checked. No safety cap was blindly raised.

Known policy gaps remain: no aggregate process/unified-memory admission budget;
no aggregate decoded shared-model byte budget; installed-package directory
discovery lacks aggregate count/byte caps (warning identities are now bounded
in the expansion below);
persistent user data and explicit unlimited diagnostic traces/recordings have no aggregate disk quota; editor serialized history accounting is
not actual `Value` heap size; and most general host/driver allocations do not
have recoverable OOM injection. On 16 GiB, measured scenes/transitions are the
validated tier. Existing 192/256 MiB optional-cache ceilings are each a small
fraction of 8/16 GiB, but shared accounting must not be summed or treated as a
hardware guarantee. An 8 GiB or discrete-GPU tier, hours-long sessions, arbitrary
custom content and a complete pressure/eviction policy require further work.
The audit is an inventory with demonstrated fixes and explicit gaps, not a claim
that issue #27's entire resource acceptance is closed.

### Deliverables and handoff

All raw receipts, `final-phase-results.json`, `hypothesis-results.json`,
`lifetime-results.json`, `extended-results.json`, `resource-audit.json`, native
logs/actions, baseline/final binaries and packages remain in
`local/performance-0.13.0/`. The editable montage inputs and builder live in
`montage/`; its seven matched cards include the regressed Pool run and unchanged
Retina views. It is 56 seconds, 1920×1080 H.264/yuv420p/30 fps. Recordings are
explicitly paced at 60 Hz; labels use separate uncapped measurements without
capture. Retina footage is downsampled only for delivery. Raw clips are retained.
Full decoding of all 1,680 frames succeeds; representative motion frames and readable labels are inspected. The confirmed video Library ID is `libfile_27a4369238608191bbab0aef74a21fe0` (`VIDEO-LIBRARY.json`).
Three inspected in-game JPEGs are already in Library: dense models
`libfile_ba6cc0561c408191b396e4882920e015`, physics/pickup
`libfile_dddc2ce782fc81918eb2cfc7f5a6c593`, and animated AI/passages
`libfile_c7039a69e1888191af367d02785ce297`.

At this checkpoint, format, strict all-target/all-feature workspace Clippy,
locked all-feature workspace release build and all **217 tests** pass. The five added regressions
use tiny scalar/channel/pose/header fixtures; no Cargo test or CI step bakes a
map. Native final-editor/game checks above pass. The two unrelated untracked
Python audit reports, old packages/receipts and `target` are preserved. No CI
wait, force push, main merge, release or credential change occurs in this pass.

### Current 27-item expansion

The first validated checkpoint was pushed as
`2c9671b2a850deb7585f082a522c3da8824f4329`. The user's later request brought all
27 ideas into this same stage. The outcomes below supersede the checkpoint's
performance summary where noted; its binaries, clips and receipts are preserved.
`local/performance-0.13.0/checklist-27.json` records each item's detailed outcome.
The measured final player SHA-256 is
`fe1f2097fa0735052d72489be965515fd301187054f9fcda7a62f4872eb3e5a5`.

| Items | Implemented or verified outcome |
|---|---|
| 1 | Unchanged dynamic matrices reuse normal transforms, full bounds and winding; independent lighting/receiver dependencies still update. |
| 2 | Cull transparent entries before sorting with that pass's actual frustum and complete posed/world bounds. |
| 3 | Bind shared dynamic geometry once/object; retain compatible decal pipelines while replacing their old-map caustics bindings. |
| 4 | Pack attached lights/flashlight once/frame; exact full-state guards suppress unchanged uploads in all targets, including bake/empty-set clearing. Existing pose/uniform guards remain. |
| 5 | Existing compatible static/character batching retained. Per-object lighting/environment bindings prevent a simple general instancing substitution; no unmeasured renderer rewrite. |
| 6 | Empty static classes skip binding setup. Existing emission-visible bloom, absent-flashlight, empty-effect and zero-index guards remain; transparent emission occluders remain. |
| 7–8 | Shadow/reflection dependency audit: preserve moving/held/posed/opacity/shadow-only visibility and live lights/materials/caustics. Prepared probes are reused; no incomplete dirty shadow/planar key introduced. |
| 9 | Bounded eight-sheet staging and persistent pose/effect/GPU buffers retained; redundant light writes removed. No unsupported staging-ring speedup claim. |
| 10 | Exact unchanged solver-pose/interpolation/trigger-path work skipped; final settling motion and rotation/reversal sweeps retained. All wake/solver/force thresholds preserved. |
| 11 | Query refresh follows both player and door changes once; static structures and unchanged Rapier setters already guarded. |
| 12 | Expanded AABBs reject distant dormant/player pairs before the original exact test, preserving key order. Existing broad-phase/swept pickup/controller/camera queries remain. |
| 13–15 | Existing goal/door/age/retry replanning gates, eight-query fairness, staggered perception, events/timers and change-only state events retained. Perception now clones only the selected contact's role and skips non-winning farther sight rays. |
| 16 | Bounded 32-slot stack schedule replaces automatic-handler path clones/allocation/search; each invocation still dispatches before the next and checks disabled/frame budgets. |
| 17 | Cache immutable validated start/update functions within their owning VM. Source already compiles once/load; resets still create isolated closures/globals. |
| 18 | Separate script timing and bounded VM allocation/automatic GC retained. No demonstrated benefit warrants changing compiler/debug/GC policy or caching bytecode across fresh VMs. |
| 19 | Existing spatial trigger tree, swept queries, occupied retry set and bounded scratch retained. No trigger-contract or polling-cadence change. |
| 20 | Share immutable navigation/grid statistics and probe faces; prepare validated navigation once/variant, upload captures then move them, and release parsed recipe wire bytes early. Collision ownership and bounded segment/PMP overlap remain. |
| 21 | Remove duplicate segment hashing: bounded descriptor-derived reads still receive complete length/SHA verification before output allocation and final PMP verification before decoding. ZIP/alias/source-recovery contracts remain. |
| 22 | Keep existing content-keyed cache budgets/eviction; charge shared backing capacities, pointees and control blocks. No cap increase or path/mtime validity shortcut. |
| 23 | Skip expensive world snapshots when neither explicit tracing nor the persistent lifecycle journal consumes the event. Embedded shaders/preloaded audio remain outside ordinary frame work. |
| 24 | Replace repeated device-loss mutex locks with first-reason `OnceLock`; normal GPU polling/timestamp readback remain nonblocking. Explicit readback/evidence waits remain. |
| 25 | Reuse transparent scratch/precomputed distances, with insertion ordinal preserving exact stable ties; release all five stores on menu. No-blend decals avoid order-vector allocation. |
| 26 | Remove duplicate lighting/transform/physics/hash/immutable-storage work above. Bound process warning identities at 4,096 keys/1 MiB UTF-8 payload, with one terminal saturation notice; ordinary warnings/fatals remain independent. |
| 27 | Verify release optimization 3/full LTO/one codegen unit/strip. Preserve Luau-required unwind. No unsupported PGO, fast-math, target or worker-count change. |

A concrete additional root cause was stationary rotated-prop interpolation.
Using an actual sleeping Bowling ball pose, the old equal-endpoint formula
produced **10 canonical model matrices and 74 consecutive changes across 257
frame fractions**, with at most 1.91 micrometres of position error. Those matrix
bits are inputs to shared visibility and receiver-lighting keys. The identity
return preserves the exact settled pose, without a movement epsilon or reduced
simulation. A 257-scalar regression protects it. In four isolated Group1→Group2
Bowling trials, recovery CPU time fell **4.17–4.99→1.04–1.05 ms** and loop p99
**8.80–8.94→3.07–3.12 ms**; bodies were asleep in both. Physics-step time accounts
for only about 0.02 ms of that difference. The isolated group includes the other
small physics bookkeeping changes, so its entire gain is not assigned to one
line. The separate headless 480-frame campaign retains exact before/after state
hashes and one/four-worker parity; its inactive-player contacts/water/dormant
throughput is effectively unchanged (128-contact serial median ~0.244 ms).

### Expanded final timings and limits

Twenty-eight new capture-free alternating trials use the original `f38c638`
baseline and the final production binary above: two runs/build in ABBA order for
each case. Settings/backend/drawable/presentation evidence follows the earlier
method: M2 Pro/Metal, Release, High/Full, VSync and limiters off, actual Immediate,
no missed ready surfaces, and measured throughput exceeding display refresh.
Static samples use 120 warmup frames then 2,400 lab/1,800 Retina frames. Interaction
runs last 12 real seconds; recovery is 6–11.8 s and closet-near is 2.4–4 s after
ready. GPU samples are deduplicated by completion ID. All means, p95/p99/worst,
stutter counts and physics/held counts remain in `expanded-results.json` and raw
receipts. No compile/recording competes with authoritative trials.

| Matched view/window | Average FPS before→after, two runs/build | CPU update mean before→after | Loop p99 before→after |
|---|---|---|---|
| Lab, 1280×720 | 388–392→429–431 | 1.95–1.96→1.75–1.77 ms | 3.30–4.47→3.13–3.14 ms |
| Home books recovery, 1280×720 | 505–537→594–600 | 1.18–1.40→0.227–0.236 ms | 2.77–5.23→3.05–3.19 ms |
| Bowling recovery, 1280×720 | 91–108→451–452 | 8.03–9.66→1.078–1.081 ms | 13.53–13.78→2.83–2.95 ms |
| Home cushion recovery, 1280×720 | 552–580→557–588 | 1.08–1.16→0.234–0.235 ms | 2.64–2.66→3.01–8.26 ms |
| Pool closet near, 1280×720 | 622–640→626–630 | 0.175–0.182→0.180–0.201 ms | 3.02–8.67→2.94–3.68 ms |
| Arcade busy, 2560×1440 | 244.05–244.62→244.36–244.80 | 1.163–1.165→1.042–1.046 ms | 5.84–6.65→6.16–7.50 ms |
| Bathroom, 2560×1440 | 367–370→364 / **209** | 1.124–1.148→1.007 / 1.211 ms | 5.02–6.62→6.69 / **11.25** ms |

The lab improves about 10.3% in throughput. Books/Bowling recovery CPU savings
are substantial; real-time trajectories differ slightly, and per-frame solver
cost also falls when fewer frames contain a fixed step. These are not changes
to the fixed-step rate, collision accuracy or sleep policy. Cushion throughput
ranges overlap despite lower CPU cost. Closet throughput is flat; its earlier
627→375 pacing pair remains unexplained. Arcade GPU span remains ~4.44 ms.
Bathroom GPU span is 4.11–4.14 ms before, 4.03 ms in the normal after trial and
4.51 ms in the regressed trial; CPU render mean rises from ~1.42 to 3.35 ms in
that regressed run. New-rendering-only repeats also showed pacing variation in
both builds, so no cause or reliable GPU-throughput gain is assigned. The
regressions are retained in evidence and montage labels.

Four matched fresh-process Lantern loads, comparing checkpoint→expanded code
with warm OS cache, measure request-to-commit **4,778–4,832→4,669–4,679 ms** and
kernel peak RSS **2,121–2,126→2,099–2,104 MiB**. Commit RSS is ~2,099–2,101 versus
~2,079–2,082 MiB. This small matched improvement is distinct from the earlier
+312 MiB observation, whose cause/lifetime remains unproven. Retained prepared
payload is 965,299,278→965,299,582 bytes; the 304-byte increase is updated shared
navigation/statistics accounting, not more scene content. Active-map RAM
reduction remains secondary to safe inactive-map release and speed/quality.

The final 318.89-second native campaign completed **42 requests/42 commits,
28 graphics-setting changes and 29 drawable observations**. At all seven menu
captures, active prepared bytes, script modules/VM bytes/calls, AI agents, physics
bodies, pending commits and retired prepared-record/build owners were zero.
The final menu GPU counts were **35 buffers, 19 textures, 35 bind groups and 34
views**, identical at all seven captures. These bounded device/UI/pipeline
resources include the three retained decal pipelines rebound to empty caustics;
fresh/reloaded lanes and mirrors now have identical pixels and draw counts.
Menu RSS ranged 613–849 MiB, ending at 752 MiB; retained allocator/driver capacity
is not called live old-map ownership. Kernel peak was 2,706,325,504 bytes. This
five-minute campaign establishes the tested plateau, not an hours-long guarantee.

A final 123.51-second Lantern→Hub→Home→Lantern→Hub→Arcade→Lantern→Hub→menu
reconfirmation on the measured player completes all eight commits. Every retired
Lantern record/build reaches zero; bounded passive Hub/Home/Arcade cache entries
release at menu, where all eight retired owners and all map subsystems are zero.
Menu GPU counts again match 35/19/35/34. Heavy RSS is 1,544/1,919/1,899 MiB,
menu 737 MiB, kernel peak 2,476,916,736 bytes. This checks the final shared
navigation/probe ownership changes; `expanded-heavy-lifetime-results.json`
retains the snapshots, rather than inferring release from RSS alone.

Final native cancellation/retry confirms a canceled Lantern load cannot commit
later or resurrect map resources. The controlled 3 MiB Luau allocation against
the existing 2 MiB VM cap still disables the failed module, discards staged
actions, repeats safely after reload and releases VM ownership on menu. Lab
contact activation/reset and visitor counts **12→13→12→13** pass; water entry,
dive, resurfacing and rim exit, and the folding door's closed collision,
traversal, inside close and mid-travel reversal pass. Walking into and stepping
onto authored dormant movable props and genuine Rapier sleepers wake them in
focused side/top regressions; fixed props remain fixed. No sleep/solver policy
was relaxed. Home cushion pickup/carry/drop and books jump/landing/recovery, and
Arcade held/contact-heavy paths pass the recorded inputs. The original severe
persistent Home collapse and Pool proximity fault are not fully reproduced or
declared resolved by these inputs.

The earlier same-view Hub comparison remains relevant to the prior-map CPU
hypothesis: fresh **583 FPS / 0.167 ms update** versus **625/636 FPS /
0.132/0.149 ms** after repeated heavy transitions, with the same 39 draws and
zero old scripts/AI/physics. Old-map active ticking did not explain a slowdown
in that tested case. Higher RSS alone establishes neither a leak nor a cause of
other pacing cases. The isolated heavy-menu cleanup callback's 52.46 ms remains
a latency lead; skipping discarded snapshots does not prove that drop cost gone.

All four final matched static Home/Pool/Arcade/lab frames have **zero changed RGB
channels** against the original baseline. The final native editor again edited,
saved, closed, reopened the edited document, reversed the edit and exported the
whole lab project normally. Restored source values match except serialization
of the existing `floor_y=0` default. Prepared export payloads are identical;
manifest/provenance fingerprints legitimately differ for the rebuilt compiler.
The existing large collection passes validation without rebaking. Separate lab
Luau invocation timing (including capability setup) is 0.0899 ms mean, 0.1285 ms
p95, 0.1362 ms maximum across its bounded 64-call diagnostic, versus checkpoint
0.0781/0.1052/0.1336 ms. There is no isolated VM execution speedup claim; automatic
schedule allocation/search occurs outside that invocation interval.

The warning-key policy gap above is now closed: key payload and cardinality are
both bounded, duplicates allocate no string, and pressure stops new identities
with one notice. It is diagnostic suppression state, not an asset-admission
limit. Hash-table/String metadata is additionally bounded by cardinality; the
1 MiB label is UTF-8 key payload, not total allocator residency. Other aggregate
process/shared-model/directory/disk/OOM/tier-policy gaps remain explicit in
`resource-audit.json`; issue #27 is not declared fully closed.

The updated **56-second, 1920×1080, H.264/yuv420p, 30 FPS montage** is confirmed
in Library as **`libfile_d5af9e225d148191b29083e5883c28aa`**. Its 1,680 frames
fully decode; sampled full-resolution frames have readable labels and the
matched actual game views. The nine new raw after clips, original before clips,
editable `edit.json`/builder, QA frames and decode/probe receipts are retained in
`local/performance-0.13.0/`; the new assembly is in `montage-expanded/`, upload
receipt in `EXPANDED-VIDEO-LIBRARY.json`. The first delivered montage
(`libfile_27a4369238608191bbab0aef74a21fe0`) remains untouched. Capture is explicitly
paced separately; all labels use capture-free timings and show overlapping or
regressed results honestly. No captured overlay supplies a benchmark number.

Final format, strict `cargo clippy --workspace --all-targets --all-features --
-D warnings`, locked all-feature workspace release build, and **222 tests**
(0.43 s) pass. Added scalar, contact, sorting and diagnostic-budget fixtures have
specific regression value and require no map bake. The final test-only rebuild
produces the exact same player SHA as the accepted timings. Gate logs are
`final-expanded-{fmt,clippy,tests,build}.log`; the actual final player, compiler,
editor and physics tool plus hashes are preserved in `validated-binaries/`.
Native receipts/results include `expanded-results.json`,
`expanded-extended-results.json`, `expanded-final-*-results.json`,
`expanded-visual-results.json` and `expanded-editor-roundtrip-receipt.json`.

The separate fresh engine sweep follows below. Remaining leads at that handoff included the
unexplained render/acquisition pacing cases, unchanged busy-view GPU cost,
residual lighting/animation work, complete safe shadow/reflection dirty keys,
PPS segment/output overlap, duplicate request-local package opening and unused
graphics-only gameplay assembly, aggregate unified-memory/model/discovery
budgets and recoverable host/driver pressure behavior. Already solved guards
and the 27 outcomes above should not be reimplemented. Mannequin ragdolls, docs
cleanup and later editor/audio/menu/UI/style work remain separate queue stages.


### Separate fresh engine sweep

This pass starts from staging `f160493f8f0f93593a7981298e61cbc12ff8f473`.
Its evidence is under `local/performance-0.13.0/fresh-sweep/`; original evidence
and the preceding 27 dispositions are preserved. Shared rendering, lighting,
geometry, physics integration, AI/Luau, loading and cache ownership were read
again; `inspection.json` records the fresh leads. Issue #31's GPU skinning/pose
sharing, portal occlusion, streaming, codec, lightmap-storage and new job-system
projects remain deferred. A reproducible Pool opening/entry CPU fault is traced
and corrected below; the independent native presentation wait remains a separate
limit and does not establish that every historical Home/Pool fault is solved.

The first three localized improvements were validated in player SHA
`8eeeda45053826f81cad1bece2382202fef410fdec0751e85b685ee61efe95a7`;
the later bounded surface/model diagnostics build is
`051ca0743777e340934bae0300fb6607bd849d52cd439a24a94da164ceb8a552`:

- Skinning reuses the last exact joint/weight blend's normal inverse within one
  skin pass. The scratch borrows an immutable animator, cannot survive a pose
  change, allocates nothing and preserves rigid-joint handling and arithmetic.
  Mannequin blended inverses fall from 968 vertices to 47 adjacent runs;
  skeleton from 626 to 151. A small deterministic fixture protects exact bits
  across different keys, poses, zero/tiny weights, invalid joints, reflections
  and singular scales; it needs no model load, bake or GPU. This is CPU normal
  preparation, with no GPU skinning, animation cadence or crowd sharing change.
- Cold loading uses one validated semantic/dependency snapshot rather than two
  simultaneous complete opens. The variant reader still independently checks
  the reopened archive's manifest, every payload hash and all existing bounds.
  No integrity check is skipped using a filename, timestamp or unchecked cache.
- Large dynamic-caster sets reject segment/AABB misses before transform/mesh-BVH
  work. A padded f64 slab guard refines the original enclosing-box guard and
  keeps original candidate order and exact triangle/alpha tests. Padding also
  accounts for large model pivots canceled by placement translation. Sets below
  64 retain the original monomorphized path after the all-size prototype added
  about 0.05 ms to the lab. Three narrow scalar/actual-triangle/canceled-pivot
  fixtures protect against false rejection without baking or loading maps.

On the same M2 Pro/Metal hardware, all timings below are capture-free with
VSync/limiters off, actual Immediate and the recorded physical drawable. High
textures/filtering/lighting, Full lightmaps/reflections, bloom and FOV60 remain.
**Immediate alone did not establish uncapped throughput in this sweep:** several
views exhibit acquisition pacing. Their FPS changes are retained as observations,
not accepted throughput gains. No recording overlays supply timings.

| Case and evidence | Before → candidate | Disposition |
| --- | --- | --- |
| Isolated normal-cache lab, CPU update | 2.149/2.148 → 2.116/2.108 ms | Small repeatable CPU reduction; FPS overlaps under presentation pacing. |
| One visible moving bowling ball, CPU update, final ABBA | 7.832/7.886 → 6.936/6.883 ms | About 12% less CPU update work. Exactly one active body throughout all selected samples; physics step remains about 0.05 ms. |
| Same moving-ball view, observed FPS at 1280×720 | 109.78/110.64 → 117.08/118.09 | Pacing-limited observation; no robust uncapped throughput claim. |
| Final lab, 1280×720 | 168.49/172.53 → 165.76/166.53 FPS | Presented regression retained despite CPU 2.198/2.204 → 2.067/2.155 ms. |
| Bathroom, 2560×1440 | 367.33/367.74 → 363.71/365.56 FPS | Flat/slightly regressed. Earlier 209 FPS fault did not recur; not declared solved. |
| Busy Arcade, 2560×1440 | 241.26/244.10 → 245.02/244.26 FPS | Effectively unchanged; GPU work about 4.4 ms remains. |
| Fixed near closet, 1280×720 | 172.25/178.81 → 179.25/184.45 FPS | Overlap under acquisition pacing; original severe fault remains open. |
| Lantern request-to-commit, fresh process/warm OS cache | 4719/4771 → 4707/4703 ms | No robust end-to-end load-time or peak-RAM reduction claim. Duplicate validated work is removed. |

`accepted-results.json` and raw per-run receipts preserve the 24-run final ABBA
set. Original real-time interactions use matched twelve-second runs and
6–11.8 s recovery windows. The initial original closet sequence regressed from
184.8/173.4 to 159.8/151.6 FPS; fixed-eye controls do not erase that result.
Bowling repeats also show why movement state must be retained: one BEFORE run
still had active bodies and CPU 6.62 ms, whereas the other BEFORE and both AFTER
runs were settled at about 1.28–1.30 ms. The earlier round had the active outlier
on AFTER instead. No general persistent Home collapse is declared solved.

Pool causal investigation is retained in `pool-cause/`. Bounded render spans
locate the largest stalls in drawable acquisition, with CPU preparation and
encoding small. Discarded frames are explicitly labeled render iterations, never
presented FPS. Enforcing GPU completion in both paths rules out CPU run-ahead as
the sole discard advantage: far/near completed discard GPU spans are about
0.61/0.90 ms, versus about 5 ms normal presented render time. This locates a
presentation cost but does not establish its cause. Eight alternating native
window controls verify actual Metal `displaySyncEnabled=false`,
`presentsWithTransaction=false`, three drawables, Immediate, 1280×720 and a 120 Hz
display. Raising the window establishes real native INPUT_FOCUS throughout the
selected frames without removing the slowdown. Interior controls also retain small CPU update cost. The open-door fixed-eye
probe initially obstructed the leaf at 29 degrees; it is not proof of traversal.
Eight SDL-owned Metal-view versus raw-host trials were flat and that prototype
was rejected. All 15 unique closet GLBs (16 placements) have finite geometry,
13,372 triangles in total, opaque double-sided materials and no exact duplicate
position triangles or degenerates. Hiding all closet model draws changes the
near submission from 81 draws/54,390 indices to 28/13,602 but leaves roughly
175–180 FPS; binary partitions and interior repeats do not identify one primary
asset culprit. This draw-only control preserves physics and baked lighting; it
does not rule out every asset/system interaction in the original fault.

An attached 12-second native Metal System Trace (successful target PID, unlike
the preserved invalid launch trace) finds 6.819 seconds of drawable-acquisition
wait in a nine-second steady window. About 4.884 seconds of those waits have no
recorded GPU execution overlap; Places GPU-stage union is 3.114 seconds. These
are merged API intervals, not a GPU utilization percentage. Native SDL/Metal
controls with no Rust/wgpu/engine/assets also wait about 5 ms for drawables.
Direct/scheduled presentation, transaction flush, event pumping and temporary
App Nap suppression do not remove it. An independent AppKit/Metal control also
reproduces the wait with real native focus. This establishes a general native
presentation limitation in this session, **not the cause of the user's original
location-specific severe closet drop**.

The user authorized one temporary display-refresh control. System Settings
verified ProMotion → 60 Hertz → ProMotion, with the original setting restored
immediately after the bounded control. At 60 Hz the empty Metal control runs
59.998/60.000 cycles/s and both full-quality Pool near/far views run 60 FPS
despite actual display-sync false. They are refresh-capped and excluded from
throughput claims. After restoration, empty controls initially run 318/324
cycles/s and focused near/far Pool views 269–275 FPS. Later ordinary-window
old/candidate ABBA overlaps around 137–143 FPS; native focus and session state
remain material qualifications. Changing overall presentation state has not
proven that the original near/far differential is fixed. Continuous original/minimal-map controls are recorded under
`pool-cause/map-isolation/`. The minimal project retains the exact closet room,
three exterior walls, shared door opening, all 18 prop records with physics and
both switches, and relevant lights. It replaces outside Pool context with a
small enclosed approach and rebakes Full lighting in that diagnostic context.
Both sources have zero geometry errors/warnings; the 2.16 MB package builds in
6.47 s, validates and verifies current. Native editor open/save/reopen passes
with exact manifest/source bytes preserved. Closed-door first approaches show
only modest differences in both maps (original 516→467 and 537→511 FPS; minimal
563→514 and 570→514). Return legs hit the minimal approach's west boundary at
X12.85 instead of the original X12.05–12.08, so those are not called matched
positions. Later camera-following open routes escape a 120 FPS presentation
plateau: original far 516, interior east 721, interior west 509 and return 499
FPS; the minimal copy also reaches several hundred FPS. None of those clear
routes reproduces the severe fault. No shipped map or asset has been changed, and no
quality reduction, frame discard or limiter is proposed as a fix.

The user's clarification—**after opening or entering**—led to a supply-contact
route at Z13.6. It reproduces a distinct CPU-heavy fault in the full original
map: far 527 FPS / 0.319 ms update, pushing/entry 140 FPS / 6.009 ms update, with
physics stepping only 0.020 ms. The minimal closet retains its supplies and
physics yet runs about 536 FPS / 0.654 ms on that segment. A bounded profile
captures about 16 ms in dynamic-lighting preparation. The earlier draw-hiding
control retained physics and CPU occluders, so it did not exclude this work.
The existing caustics-off diagnostic only suppresses displayed focus; it still
prepares visibility and is not an isolation of this CPU path.

**The fourth shared-engine improvement corrects unnecessary water visibility
invalidation.** A door or movable prop changes one global visibility revision.
Previously every active water-light source then revisited its supported cells
and possible movable blockers, even when all changed objects were outside any
light-to-water path. Pool has 65,120 receiver records and 209,420 sparse factors.
Props do not contribute light here; they are possible blockers. A padded bounds
rejection before the unchanged full separating-axis certificate first reduces
this profiled preparation to 4.0–4.1 ms. The final change retains the latest
revision's old/new changed-caster bounds plus the field's receiver envelope.
Each source reuses its existing certificates only when its finite emitter hull
or infinite directional prism is disjoint from that changed geometry. Moving
out includes the old bounds; admission/removal conservatively includes former
and current casters; a consumer missing revisions performs a full refresh.
Source activation retains its existing full invalidation. No map/asset IDs,
quality changes, throttling or deferred architecture are involved. These new
dependency records are scalar bounds/revisions, with no new retained map owner
or per-frame heap allocation.

Isolation compares the earlier core checkpoint `051ca074…` with final player
`1e5629935f032aa07dd8aee68a18a2da80cb2015643a2acd89823687827349d6`:

| Matched native control, 1280×720 High/Full | Before → final | Evidence and limit |
| --- | --- | --- |
| Door-only update, 128 predetermined motion frames/run, ABBA | 16.987/16.864 → 1.719/1.768 ms | About 90% less update CPU. Every selected frame has zero active physics bodies; physics step is about 0.001–0.002 ms. |
| Door-only observed FPS | 59.26/59.52 → 136.12/136.44 | Settled controls vary 121–151 FPS; presentation pacing remains, so this is not a general uncapped-throughput multiplier. |
| Original contact-route update during pushing/entry, ABBA | 11.726/11.624 → 1.658/1.227 ms | Normal real-time input and full props/physics; positions and body state retained in raw receipts. |
| Same entry observed FPS | 69.6/69.8 → 124.2/123.6 | Far views sit near a 120–128 FPS presentation floor. The CPU cliff is removed; the platform wait is not. |
| Focused off-region water visibility diagnostic | About 16 ms combined preparation initially; 3.43–3.52 ms water visibility after bounds rejection → 0.002–0.005 ms water visibility final | Ordinary dynamic receivers remain about 0.8–0.9 ms in the last profile. Diagnostic logging is excluded from authoritative timing runs. |

The final player's lab/one-body recheck against `f160493` also retains the
CPU gains (`final-journal-timings.json`): lab 2.356/2.340 → 2.280/2.284 ms;
one moving visible bowling ball 7.913/7.907 → 6.973/7.093 ms, with exactly one
active body in every selected sample. Observed lab FPS is 141–143 → 144–145,
and moving-ball FPS 109 → 115–117; acquisition pacing still qualifies those
throughput observations. The older flat/regressed Retina and interaction sets
above remain recorded rather than being replaced with only favorable trials.

A few thousand scalar boxes compare bounds rejection with the existing full
certificate across tiny/large scales and large translations. A two-triangle,
0.4 m water fixture checks both finite light and sunlight bases, unrelated
motion reuse, rotation, scale, movement in/out, hidden/restored/despawned
blockers and missed revisions against complete fresh field samples. It uses
no assets, GPU or atlas bake. Ten deterministic native High/Full captures—five
Pool states and five Aero circular-water/sunlight states—have **zero changed
RGB channels** versus the earlier core checkpoint: initial, off-water move,
over-water placement, move out and despawn. The actual prepared fields contain
209,420 and 1,650 factors respectively. These checks protect the supported
visibility calculation; they do not promise that arbitrary future in-water
workloads are free.

Both the core implementation and later diagnostic source pass format, strict
all-target/all-feature Clippy, locked workspace release build and **228 tests,
zero failures/ignored**. The final `final-integrated-*` logs record 0.40 s tests
and a 3m17s locked workspace release build; all four executables and SHA256 values
are preserved in `final-validated/`. The final built player is byte-identical
to the one used for the accepted Pool controls. Default-feature compilation is included in the
workspace tests. Earlier failed diagnostic feature-gating logs are retained
and superseded by this successful run. Four matched lab/Home/Pool/Arcade frame-180 captures
have zero changed RGB channels. Native lab controls retain 25 contacts, activate
and reset the intended props, and spawn/reset the visitor. Luau's bounded 64-call
host-inclusive diagnostic averages 0.106 ms, p95 0.189 ms, max 0.209 ms; no isolated
VM speedup is claimed. Folding-door obstruction/traversal/reversal, Pool water
entry/dive/resurface/rim-exit/reentry, original cushion/books/bowling interactions,
loading cancellation, graphics restore and allocation-failure checks pass.
The lab source is unchanged; this pass relies on the preceding native editor
edit/save/reopen/reverse/export proof rather than claiming another editor pass.

The earlier eight-transition heavy-map cycle completes all eight requests and
commits. Every retired prepared owner is zero at menu; active prepared bytes,
script modules/VM, AI, physics and pending loads are zero. Final settled menu GPU
counts are 35 buffers/19 textures/35 bind groups/35 views (one transient buffer
persists in two earlier tail samples). Active Lantern prepared data remains
965,299,582 bytes, accepted by the user. Heavy RSS samples are 2023/1972/2265 MiB,
menu 1016 MiB, peak 2418 MiB. This is not monotonic growth evidence, and neither
this menu residency nor the historical +312 MiB has a proven cause. Map owners
releasing does not prove all allocator/driver residency is explained. Metal HAL
byte counters remain unavailable. Existing process/shared-model aggregate
budgets, discovery directory/disk quotas, general recoverable allocation failure
and 8 GiB/discrete-hardware gaps remain explicit in the original resource audit;
arbitrary active-map RAM reduction is not substituted for a demonstrated defect.

On the final `1e562993…` player, lab contacts activate/reset and the Luau visitor
again follows 12→13→12→13; Pool water entry/dive/resurface/rim exit/reentry and
Home carrying pass. A final Pool→Aero→Hub→menu check leaves all three retired
prepared-build/record owners at zero, active prepared bytes zero, no scripts/VM,
AI, physics, pending commit or preparation. Worker caches report prepared,
identity, image and model counts zero. Menu GPU tail returns to 35 buffers,
19 textures, 35 bind groups and 35 views after transient capture/command buffers.
Menu RSS is about 595 MiB in this shorter run; it does not explain the earlier
menu residency or historical +312 MiB. Final receipts and state witnesses are
in `final-native-checks.json`; no new ownership-bearing cache was introduced.

Fourteen new matched short raw clips and editable assemblies are preserved
outside `target/`. The final decoded and visually inspected **112-second**
montage keeps the original 56-second preceding montage and six core-sweep cards,
then appends the newly matched Pool opening/entry pair with CPU savings and
pacing qualifications. Local file:
`fresh-sweep/montage-final/places-0.13.0-performance-fresh-engine.mp4`,
1920×1080 H.264 High/yuv420p, 30 FPS, 3,360 frames, 7,199,587 bytes, SHA256
`d851a7e6150f7cdfc78e13433ea6b693d780c25830bd281f42025f165e50623e`.
It replaces only this sweep's checkpoint item as **Library
`libfile_bb4cac714b748191aeb7324d98396e43`, version 1**, file
`file_00000000d624823091171dd807ae54ba`; a read verifies its ID/name/size/version,
and its local Library xattr is verified as 1. The checkpoint's version-0 local
video/receipt and all raw evidence remain. A read also confirms the untouched
preceding Library `libfile_d5af9e225d148191b29083e5883c28aa`. Captures are paced
at 60 Hz with 30 FPS output; numeric panels use separate capture-free runs.

No deferred issue #31 project, shipped map, asset, physics policy or integrity
check is changed. The reproduced Pool CPU fault has a validated shared-engine
fix; the native OS/Metal presentation wait and broader historical Home/Pool,
RAM-residency and hardware-tier limits remain explicit. CI is owned by the
final integrated queue stage and is not awaited here. The next stage is
editor/audio polish, then main-menu/visual styles; mannequin ragdolls is second
to last, before docs cleanup/final integration. The approved stair-generation
and structural-thickness work is recorded for that next owner, not implemented
in this performance pass.

## Editor and audio polish stage — 2026-10-21

This stage starts from `6c1127ff869d5c0c44f06725efd70b489c708ed6` on
`0.13.0-staging`. The single writer retained the previous performance evidence,
packages and two unrelated untracked Python audit reports. Issues #18 and #29
and their comments were read; completed 0.12 authoring work was reused. Issue #31
architecture projects remain deferred. No main merge, release or CI wait belongs
to this stage. Cargo's release version remains the queue's existing 0.12.0 value;
this work does not independently change release metadata.

### Delivered behavior

- **Visual selection:** 525 indexed, committed 160×120 PNG previews cover the
  actual prop/entity GLBs, catalog tint, material images, six native fixture
  bodies and all three native door styles. Eight-tile grids retain names, theme
  filtering, search, paging, keyboard access and missing-preview feedback.
  Room material selection distinguishes Floor/Ceiling. Door styles and door
  materials preserve the rest of the authored instance. A visible tile is decoded
  at most once per frame; CPU artwork retention is capped at 24 tiles (1.76 MiB),
  and the existing GPU UI image cache remains capped at eight. Offline generation
  uses shipped assets; the editor never renders the entire catalog continuously.
  `--with-editor` distributions include the preview files and index.
- **Bake review:** Project → Bake status uses the installed compiler's dependency
  and integrity contracts on a background worker. It considers both individual
  outputs and the complete export, with valid / metadata refresh / stale rebake /
  invalid or unknown states, causes per row and paging. Valid rows cannot start
  redundant builds. An affected row builds only its selected map. Existing
  compiler progress, cancellation and atomic publication retain the last usable
  package. The size review counts physical shared project blobs once, classifies
  their virtual roles, and explicitly distinguishes ZIP archive bytes, entry
  bytes before internal codecs, installed cache and RAM. Index/size work also
  runs off the UI thread.
- **Structure:** the shared default is **0.15 m** in the existing metre units,
  compared with the previous 0.12 m rigid terrain backing. Floors, raised regions,
  slopes and stairs have actual downward faces and exposed edge bands from the
  same convex patches as collision. Existing solid ramp/stair wedge semantics
  remain. Equal internal seams and wall-covered bands are clipped, and lower
  ceilings own matching underside material. Player head contact uses that same
  backing. Geometry revision 4 intentionally requires older prepared geometry
  to be rebuilt; the planner must not call that global contract change unrelated
  per-level motion.
- **Stack safeguards and Annex:** Safety/export reject insufficient lower-ceiling
  to upper-floor clearance, including a ceiling intruding into the upper occupied
  room. An enclosing hall roof above several storeys retains its ownership.
  Safety provides an exact-source, reviewable ceiling proposal with adequate
  standing clearance; nearby explicit flush-fixture drops are retained in the
  same undoable proposal. Nothing silently moves a room or fills a shaft. Add
  Upper Floor places the new floor above the unchanged lower ceiling. Tower
  rooms 0–2 keep their walking elevations and have 3.05 m ceiling height; three
  redundant landing ceilings inside the tall stair hall are open. Only the
  corresponding three flush-fixture heights are reanchored by 0.15 m. Existing
  ramps, stairs, passages, floor elevations and intentional layering remain.
- **Stairs:** Add Geometry → Connect two floors selects the lower/upper rooms and
  width. The conservative straight-flight workflow supports overlapping flat
  floors, 0.9–4 m width (1.2 m default), risers at most 0.2 m, 0.28 m treads,
  landings at least width/1.2 m, a real cut shaft and upper standing clearance.
  It checks the flight/landing envelope against walls, transformed prop bounds,
  fixture footprints, door swing envelopes and intervening floors. Ambiguous
  global terrain or insufficient fit gives an actionable refusal. Preview/Cancel
  preserve source; Apply creates ordinary editable room/stair records in one
  undo step. Manual Staircase and numeric controls remain available.
- **Editing:** colored viewport axes translate props, fixtures and isolated rooms
  with toolbar snapping, private previews, Escape/focus-loss cancellation and one
  history commit on release. XYZ labels share the actual red/green/blue viewport
  colors. Isolated-room moves carry supported contained records and fog boxes;
  shared boundaries, crossing boxes and coordinate-bearing global geometry/scripts
  require explicit review. This is not arbitrary connected-room group dragging.
  Existing per-map history bounds (64 revisions / 32 MiB per undo or redo stack),
  coalesced preparation and cache pruning remain; diagnostic receipts now expose
  retained counts and preview latency. Safety remains explicit/background work,
  with no continuous all-pairs validation added.
- **Audio:** Kira 0.12.5/SDL3 is retained. The 63 WAVs now supply 25 cues; a restrained
  `water_transition` reuses recorded splashes for real swim entry/exit. Loading
  and reset do not manufacture a transition. The unusually loud third switch
  and plastic takes have reproducible post-normalization gains of 0.65 / 0.5;
  source identity, trim and edge fades remain. Provenance and packaged WAVs are
  updated. Standalone emitters expose gain and optional near/far attenuation;
  authored/triggered playback carries the same override. Preview supports
  stop/replay and is cleared on Back, assignment, close and map replacement.
  Missing references remain named validation errors, and voices retain the
  engine's bound of 48.

### Acceptance and limits

The stage's authoritative logs, exact package/binary identities, native source
fixtures, screenshots, recipes and receipts are under
`local/editor-audio-0.13.0/`; the final index and ownership state are in
`FINAL-HANDOFF.json`. Deliverables remain outside `target/`. Tiny deterministic
regressions protect stair shaft/landing support and undo/save/reopen, structural
undersides matching collision, reviewable ceiling/fixture correction and room
fog translation/refusal. They perform no shipped-map bake. Existing trigger,
fog, door, physics, placement-intent and shared water-lighting regressions remain
in the owning suites.

Native audio campaigns pass controls (37 checks), lifecycle (43), settings (28)
and movement/material footsteps (32), with real post-mixer PCM and voice traces.
The actual master-zero restart records saved master `0.0`, zero active/new voice
gains and 64,512 stereo PCM frames with exact zero peak. The next menu stage must
reuse `Settings::set_master_volume`, normal settings persistence and the shared
`AudioSettings::master_volume` mix path for main/pause settings. No new main-menu
scene or second master bus was built. These observations do **not** establish
subjective sound quality or physical device hotplug behavior; neither was tested.
Browser audio and other desktop platforms were not part of native acceptance.

Native generation exercised Preview/Cancel/Apply/Undo/Redo, save/reopen and export;
ordinary W/S movement, with noclip off, reached and returned from the generated
3.35 m landing. Native drag receipts distinguish actual cancellation after
reselecting objects following undo; all three cancellation sources are unchanged.
The 80-edit history run retains 64 revisions / 33,102 serialized bytes, and after
64 undos retains 64 redo revisions / 33,103 bytes. Its native peak footprint was
371,967,224 bytes; serialized history accounting is not process RAM.

Initial diagnostic failures are retained: catalog/transition/script admission
correctly refused incompatible baseline packages; corrected geometry-only
baseline fixtures retain compiled scripts and remove transition admission only
in the copied diagnostic source. One baseline capture run had two unavailable
surface frames and is excluded from throughput evidence. Early UI recipes that
requested hidden scene controls or used Close instead of Back were corrected;
an extra diagnostic nudge correctly produced an unsaved-close dialog. None was
used to bypass an access denial, and no user project was discarded.

The matched seven-map editor baseline did not show a general map-switch speedup:
ready times before/after were Hub 399/416 ms, Home 413/414 ms, Pool 2700/2780 ms,
Office 397/399 ms, Arcade 1246/1148 ms, Lantern Hollow 5806/5893 ms and Winter
1470/1465 ms. One short native peak-footprint sample was 3,095,349,408 versus
3,069,003,912 bytes; that is not a general memory improvement claim. A subsequent
nine-member project loop completed twice and stabilized at nine documents and
69 decoded images, returning to each map's same retained model count. Its peak
footprint was 3,118,221,472 bytes. Serialized history and image cache bytes are
not total process residency.

The retained Pool UI stall was investigated before handoff. Native sampling with
symbols traced it to `Caustics::upload → ReceiverField::build`, including source
visibility work on the UI thread. The editor now prepares that exact CPU field
on its existing coalesced preview worker and attaches it to the same immutable
prepared scene. No GPU handles cross threads, no new scheduler/streaming project
was started, and the ordinary player path retains the existing preparation.
The final matched, unsampled release pair records the longest UI upload handler
**1,595.38 → 78.56 ms**, while total request-to-ready stays approximately unchanged
at **2,828.78 → 2,860.48 ms**. Both retain 149 viewport objects, 38 models, 43
images, 19,136,512 model texture bytes and exactly 65,120 water receiver samples /
208,716 sparse factors / 1,398,100 mask bytes / mean 0.140651. The new worker field
cost is 1,547.09 ms. The symbol witness locates receiver preparation on that
worker. The remaining ~79 ms upload span, large-map CPU preparation and
multi-gigabyte residency are explicit limits; broader issue #31 work stays deferred.
These editor operation measurements use Fifo at physical 2880×1632 and the
editor's normal pacing, and are **not FPS throughput evidence**. Player captures
request Immediate, retain actual drawable/settings in receipts, and show the
known ~120 Hz presentation plateau; this stage claims no engine FPS gain.

A native two-level bake review demonstrated metadata refresh, both current,
then only Workshop requiring a bake while Second stayed current; checker and
build cancellation retained usable output. An additional export-only regression
selects the **non-entry** Second member with no standalone output, changes only
its name and builds it. It reports a verified `prepared_world` hit in 51.84 ms,
with no illumination solve or probe capture, publishes a current standalone
package and leaves the complete export byte-for-byte unchanged. Compiler reuse
now resolves project member records before checking provenance and can read the
standard complete export as a read-only cache fallback. The tiny existing
project-archive fixture also covers this virtual record selection. Structural
Safety proposals share one immutable source and retain at most 256 proposals;
full candidates are constructed only when reviewed, avoiding a full document
copy per stacked-room pair.

Annex Tower's explicit geometry check reports **zero errors / 48 retained
heuristic warnings**: 14 missing-wall runs, 31 reversed faces and three room-leak
samples. No new placement ignores or broad suppression were added. Native final
views inspect the SECOND ROOM ceiling, flush fixtures, exposed upper-floor edge
and real ramp/stair backing. The final three-level export passes integrity and
current-source verification for every member, and import preserves its project,
three source JSONs and three Luau scripts byte-for-byte. Full collection rebaking
is reserved for final integrated validation after subsequent stages.

Actual native audio receipts cover a triggered, authored emitter at gain 0.6
with 0.5/6 m attenuation, preview stop/replay/Back cleanup, and door/physics/
trigger/fog authoring controls. Its far voice gain is exactly zero while the
near voice has positive mixer gain; unrelated attached water retains its
catalog falloff. A finished Discharge basin/ramp route produces real entry/exit
cues and finite nonzero post-mixer PCM. Continuous ordinary movement exposed
brief swim-state oscillation on the ramp; a 750 ms real-time sound retrigger
guard suppresses repeated splashes without altering collision, water physics or
queuing audio. It resets on unload. Diagnostic pose-assisted and continuous
routes, including the initial repeated-splash witness, are retained separately.
The final continuous route uses W/D/S from the initial dry fixture with no
intermediate player pose, observes the entry and exit sound without subsequent
retrigger chatter, reaches grounded dry support with noclip off and records
492,544 stereo PCM frames (peak 0.221519). Shutdown reports zero live/queued
voices, dropped requests and errors. No subjective listening claim is made.

The final macOS distribution includes the player, compiler and editor in flat
and app forms, with every one of the 63 WAVs and 525 previews byte-identical to
source. The packaged editor opens from `/tmp` with no asset-root override and
exercises paging, search, no-match feedback and ceiling materials; 12 page
checkpoints retain at most 16 decoded tiles. The first package recipe searched
an authored prop's ID as though it were its model label and was corrected to the
existing Workshop scene; the failed log remains. No user content was removed.

Six native screenshots and a 12.97-second, 389-frame silent stair traversal demo
are preserved in `local/editor-audio-0.13.0/deliverables/`. Library saving failed
during preparation in the required batch workflow, including one normal-network
retry; it returned no confirmed file IDs. The local deliverables and failure
logs remain ready for the parent to save later. No alternate upload path was
used after starting the required workflow.

Remaining acceptance boundaries are explicit: generated stairs require the
supported overlapping-flat-floor straight-flight fit; connected-room group
moves, arbitrary stair layouts, subjective audio balance, physical device
hotplug, browser audio and other desktop platforms were not validated. Main/pause
master-volume UI belongs to the following menu stage. Mannequin ragdolls remain
second to last, then docs cleanup/final integration. No main-menu scene, main
merge or release was made here.


The final gates pass: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace` (232 passing; no shipped-map bake), and
`cargo build --locked --release --workspace --all-features`. Native structural
review additionally confirms Preview/Cancel leave source exact, Apply changes
the lower ceiling 3.0→2.85 m and flush fixture 2.98→2.83 m together, Undo restores
the original, and Redo/Save retain the reviewed proposal. Rejected export of the
zero-gap source leaves its previous package hash unchanged. The scope page is
advanced before choosing the measured finding; the initial mistaken recipe is
preserved. Staging commit and verified remote identity are recorded in the
on-disk handoff after publication; CI remains the final integrated queue's job.


Final local deliverables are `Relay-Annex-Complete.placesmap` (23,465,925 bytes,
static and full variants), `Sound-Authoring-Complete.placesmap`, and the flat/app
`payload-complete/` distribution under the stage evidence directory. All older
stage packages remain. Their SHA256 identities and exact release binaries are
in the final handoff. The next menu owner can start at
`src/game/settings.rs:1260` and `src/game/app/audio_runtime.rs` for the existing
persistent master control/mix path.


## Main menu, miniature and visual styles stage (historical)

This stage was superseded by the [Pool, menu and shared dependency follow-up](#pool-menu-and-shared-dependency-follow-up--2026-10-21), including seven complete events, sparse staging, removed slogans and recovered Library delivery.

Both supplied PNGs were inspected as actual pixels. The UI original is the
preserved local image from “Draft Places menu concept art”; the scene reference
was materialized on this Mac through the Library download helper. The native
menu uses the reference's cream/yellow restrained palette, compact pixel Places
wordmark, slogan and left-side Explore, Settings, Credits and Exit. The wordmark
is composed with the committed font atlas; three unsuccessful generated alpha
cutouts remain local and are not shipped. There is no main-page level carousel.
Explore has a separate four-card grid, actual package thumbnails, a font-atlas
fallback, full paged descriptions, pinned import/back actions and keyboard
navigation. Its metadata/image reads do not prepare the collection's worlds.

The application owns an independent 8.02 MB prepared scene at
`assets/shared/menu/scene.bin`, authored in `projects/menu`. Office, Pool, Home
and Arcade are its four spatial depth layers. It uses the rebuilt catalog,
a recessed flowing pool with caustics, practical fixtures, six mannequins,
five pickup items, three reusable buoyant ducks and the real folding closet.
The camera is fixed at `(0,1.6,3.5)` with a 55° vertical lens. The resource is
packaged under shared assets and does not need any gameplay package or checkout.

The Luau module contains 120 five-second cues across twelve vignettes: delivery,
books, towels, a cushion/closet inspection, Arcade prizes, duck flotillas,
greetings, coordinated crossings and closing rounds. These are 120 chronological
beats, rather than 120 unrelated complete encounters. One pending contact action
aligns a real pickup/putdown clip with transfer at 0.9 seconds. Carried objects
use posed hand sockets; putdown restores the same key, mass, scale and authored
reset position to the ordinary solver. The menu-only pickup rule cannot bypass
normal gameplay's held-item and reach requirements. No script spawns entities
or accumulates timers. Reset at 600 seconds cancels the VM's pending work and
restores actors, item groups, closet, lamps, navigation and audio.

Main/Explore/settings advance only the menu world; gameplay replaces its
simulation and GPU resources and explicitly releases worker cache ownership.
Pause/settings retain gameplay. Return unloads gameplay through the recent
cleanup path and loads a fresh menu scene. Existing lighting invalidation is
preserved. The basin review exposed overlapping upper structural bands and
terrain skirts; correcting face ownership removes the duplicate without
changing collision backing. Geometry revision is now 5. The menu geometry
check reports zero errors and zero warnings. The first full native cycle caught
an unreachable Pool starting position and an off-camera closet; final authoring
clears the attendant, moves the Home alcove into view and separates the Office
desk from actor traffic. The original full-cycle recording is preserved. Existing gameplay packages remain
readable; final integration owns rebaking those sources against the revision.

Standard, CRT, Legacy and VHS are separate persisted presentation styles. CRT
adds scanline/mask/vignette treatment; Legacy supplies a warmer reduced display
palette; VHS adds chroma/time-base/head-switch/noise treatment. They use the
existing postprocessing path and leave quality, textures, lightmaps, reflections
and bloom independently selected. Legacy is a display treatment, not a revival
of an older lighting solver. Main and pause Audio pages share a draggable master
slider and keyboard adjustment, routed through the already verified persistent
Kira master gain. No competing audio volume path was added.

Errors retain their complete message, wrap long path components and page with
reachable dismissal controls. Native inspection includes an 858-character path
at 480×270 logical / 960×540 physical pixels, both pages and dismissal. Other
receipts cover zero maps, missing thumbnails, 64-member browsing to the final
card, paged descriptions, resize, cancellation, Credits and two full menu →
Explore → play → pause/settings → return round trips. Gameplay has zero menu
script modules; paused settings keep the same gameplay world.

Final acceptance ran for **611.004 seconds**, observed beats 1–120, reset at
600.611 seconds from launch, and continued into the next cycle. The 1600×900
recording contains **1,217 frames at a nominal 2 fps / 608.5 encoded seconds**;
it samples the actual uncapped renderer every 500 ms and is not an FPS test.
Chronological dense frame sheets cover the whole cycle, with separate full-size
pickup/putdown, Home and final-reset views. All six actors move; all five carriers
succeed. Receipts record 19 handoffs (including the next cycle), 14 physical
putdowns, no script diagnostics, one module, at most eight physical bodies,
567,360–649,160 sampled VM bytes and zero sampled item penetration. Peak sampled
process RSS over the full trace is 474,120,192 bytes; this is not total GPU/system
memory. No objects or script histories accumulate. The original two recordings
remain alongside the final one.

Final native checks repeat both gameplay round trips and confirm zero retired
menu owners during gameplay and zero retired gameplay owners after returning.
Pause/settings preserve the gameplay clock/world; native focus diagnostics stop
and resume the menu clock. Main and pause Audio show the shared master slider;
restart retains zero, with **260,096 float PCM samples all exactly zero** while
effects/ambience remain 0.8/0.6. Style selection through the UI persists VHS and
retains High quality and Full lighting. The final flat distribution runs from
`/tmp/places-menu-ui-0130-shipping/Places/places`, cwd `/tmp`, with no asset-root
override and zero gameplay maps. Flat/app distributions, compiler and editor are
retained in `local/menu-ui-0.13.0/payload-shipping/`.

The final separate 1,800-frame native cost sample uses actual Immediate
presentation, swap interval 0, 1600×900 physical / 800×450 logical pixels,
High quality/textures, Full lighting/reflections and bloom, with no limiter,
recording or capture. It observes a 1.713 ms mean loop (583.74 frames/s),
0.431 ms update, 1.120 ms CPU-side render submission/wait and 1.394 ms GPU pass
span mean (1.370 ms median, 1.487 ms p95). This is a short tiny-scene sample,
not an engine-wide result or comparison. Earlier otherwise matched samples
plateaued near 120 Hz despite Immediate and are retained as inconclusive;
the presentation change is not attributed to an engine improvement. GPU spans
include scheduling stalls and are not active utilization.

Final gates pass: `cargo fmt --all -- --check`, strict all-target/all-feature
Clippy with `-D warnings`, `cargo test --workspace` (238 passing; 0.44 seconds of
fixture execution, no shipped-map bake), and the locked release workspace build.
The final menu package passes dependency integrity and `verify --require-current`;
its fingerprint is `4b70a6f853575ad79cfdbd2238c47ffff053bc9250b565ae03c4f164dcab4a52`.

Library saving failed at preparation in the required unchanged batch-helper
workflow: **`library upload failed: prepare_uploads failed`**. The one ordered
19-file batch included this stage's ten screenshots, cycle overview and complete
video plus the seven preserved editor/stair files (their one authorized retry).
No confirmed Library IDs were returned; no alternate transfer path or additional
retry was used. The 43,959,967-byte local delivery set, exact request/result/log
and delivery-status JSON remain under the stage evidence directory. This is the
remaining delivery blocker. Other boundaries are the font-atlas reconstruction
rather than a shipped raster logo cutout, nominal 2 fps diagnostic video,
unvalidated Windows/Linux native behavior, and final integrated CI/collection
rebakes reserved for the following queue stage.

All evidence is under `local/menu-ui-0.13.0/`, outside target. Earlier failed
runs, generated cutouts and previous stage packages remain. The two unrelated
untracked Python audit reports retain their exact SHA256 identities. No CI wait,
main merge, release, force push, credential change, ragdoll work or architecture
project #31 was started. Ragdolls remain second to last, followed by documentation
cleanup and final integrated checks. Exact commit, verified remote identity,
binary/package hashes, confirmed Library IDs or delivery blockers and ownership
release are recorded in the final on-disk handoff after publication.

## Articulated mannequin ragdoll stage — 2026-10-21

Completed against menu-stage baseline
`dca83d4b2c52dc07e1b9c33ede22dd158a956a7f` on `0.13.0-staging`. This is the
approved second-to-last stage. Final documentation cleanup, integrated validation
and CI remain the following owner's work. No shipped map, mannequin model/PNG,
menu choreography, material quality or architecture project #31 was changed.

### Engine, ownership and use

The existing mannequin now transfers its current displayed skeleton into the
ordinary Rapier item world: **15 bodies, 14 real constrained joints and 68 kg**.
The inspected GLB has 39 nodes and 37 skin-joint slots; pelvis, spine, head/neck,
upper/lower arms, hands, thighs, shins and feet have physical segments. Fingers
and the head descendant follow their physical parent with their entry local pose.
Mapping uses node identity and the actual skin-slot order. Unsupported hierarchy,
proportions, transforms, scales or skin mappings fail before allocation.

Pelvis/chest boxes and fitted capsules retain full world, stair, prop, item,
door and player-proxy collision. Each skeleton excludes its own internal contacts;
different mannequins collide. Joint translations are constrained, all angular
axes have finite anatomical limits, and an already violated authored entry angle
expands that bound to preserve pose continuity. CCD, friction 0.65, zero bounce,
linear/angular damping 0.35/2, eight additional solver iterations and overdamped
120 Hz joint compliance produce bounded contacts. A full quiet second with the
0.1 m/s normalized Rapier sleep threshold allows resting islands to sleep;
ordinary item contacts wake them. Stronger damping of the constraints resolved
the raised-arm wall fixture's persistent small-hand rotation without reducing
collision or visual quality.

Current/previous displayed bones and actor placement provide bounded inherited
linear/angular velocities, including submillisecond render frames. Physics owns
the pose exclusively after activation: the actual AI agent leaves the active
controller/perception sets, movement/sequences stop, and animation actions cannot
reclaim the bones. Skinning, inverse-transpose normals, lighting, shadows and
reflection draws use the existing character path. Bind-space joint-influence
boxes cache a conservative posed visibility/lighting envelope without another
full vertex-skinning pass. The recent normal cache and draw sharing remain intact.
Interaction bounds and script position track the physical envelope/live pelvis.

Use `/ragdoll floor_actor on 1.5 0 -0.4` and
`/ragdoll floor_actor reset`, an ordinary JSON `ragdoll` binding, or sandboxed
`api.ragdoll(id, enabled, vx?, vy?, vz?)`. Optional additive velocity is finite and
at most 15 m/s. Reset removes bodies/joints/caches and restores a fresh authored
animation clock, placement, routes and controller. Runtime-spawned actors retain
their authored spawn transform too. Reset/reload is explicit; no stand-up or
recovery animation was implemented. Eight ragdolls share the existing 128-body
item/bone budget; duplicate/over-budget requests allocate no partial rig.

A carried item returns to ordinary physics at the current posed socket before
the controller changes. Failed release rolls back the unchanged entry pose.
The native carry trial preserves the exact cushion key `9223372036854775817`,
removes its attachment and retains one ordinary item body, with no hidden carrier
or orphan. Individual actor reset leaves that released item physical; map reload
restores the authored world. One contact-onset sample has 9.25 cm of item/hand
collider overlap inherited from the grip; the capture 0.45 seconds after release
is clear and the actor/item settle without explosion. This is a transient socket
release overlap, not a claim of zero penetration at every solver tick.

### Native acceptance and focused gates

`projects/ragdoll-lab` is a separate ordinary editor project with eight existing
mannequins, routes/AI, a six-step stair, table, lamps, reflective floor patch and
carry cushion. Console, E bindings and its Luau module exercise production paths.
`tools/bench/ragdoll.py` supplies explicit native acceptance outside Cargo/CI.
Source project checks, geometry (zero errors/warnings), package dependencies,
integrity and `verify --require-current` pass. Complete project export/import
round-trips the manifest, level JSON and Luau bytes exactly. The final editor
opens the project cleanly and its Scripts Check passes with `dirty:false`.
Preserved executables need their normal asset-root override when run outside a
distribution; initial missing-asset diagnostics are retained beside corrected runs.

All nine final native runs exit zero on the final locked release player,
SHA-256 `eddc3adc3fdf60be60aa482adc5bab35b1e8b149ea82f1901a76dabeafd70ea6`.
The final lab package SHA-256 is
`8267320450c585f3d2ae98e1aa46e11c119a2ba4bdfe99263b3ddaa6f7785d9b`;
its current fingerprint is
`5bf25fc785b572387b9408fb10dd8499358c129ed451deba8ca8d89e35ac884e`.
Contact/multiple views use diagnostic camera/noclip placement; activation and
ownership changes still use ordinary console, authored bindings and Luau paths.

- Two animated → fall → settle → reset cycles both reach 15/15 sleeping segments,
  with settled joint error 0.32/0.21 mm. Wall, stair and table trials also sleep
  all segments, with settled errors 0.41/0.44/0.96 mm and zero sampled resting
  core penetration. Native screenshots show connected limbs, preserved normals,
  lighting and moving contact/shadow bounds; the reflective floor pass remains on.
- Three eight-mannequin cycles perform 24 activations and 24 resets. Eight duplicate
  requests are refused. Peak ownership is 122 solver bodies (120 bones, one item,
  one player proxy), 112 joints and 312 cached pose nodes. Reset returns to two
  bodies, zero joints/cached nodes and the original two AI agents. The early stress
  capture still has two moving actors; it is not an all-sleep claim. In the separate
  extended timing run all 120 bones sleep by 34.16 seconds from ready, about
  29.1 seconds after activation, and stay asleep through the reset at 40 seconds.
- Physics pause holds identical bone positions and fixed-step count 31; one
  explicit step advances exactly to 32. Resume, Medium/High graphics rebuilds
  and ordinary pause-menu suspension retain the same articulated ownership.
  Actual map unload records zero bodies, colliders, joints, pose caches, script
  modules/VM bytes and retired prepared owners. The fresh menu has zero ragdolls
  and normal cast controllers; reloading the lab restores its authored actors.
- Rigid/unknown targets and nonfinite launch velocity leave baseline resources
  intact. No script diagnostics occur. Full native evidence includes ordinary
  carry/socket release, unsupported commands and the final editor script check.

Final formatting, strict workspace/all-target/all-feature Clippy with
`-D warnings`, `cargo test --workspace`, and
`cargo build --locked --release --workspace --all-features` pass. **245 tests pass**
(238 existing + seven focused regressions; 0.49 seconds fixture execution).
The new tests cover displayed-pose/skin-slot continuity, angular/translation
constraints, contact stability, sleep/contact wake, pause/single-step/retirement,
atomic unsupported/budget rejection, actual AI/reset ownership, conservative
weighted-vertex bounds, table chatter and raised-arm wall settling. The focused
seven execute in 0.26 seconds using the installed rig and analytic room/box/wall
fixtures, with no shipped-map bake or GPU. Asset validation passes 621 assets,
zero warnings; Python syntax and `git diff --check` pass.

### Idle, active and sleeping cost

Separate 45-second native runs contain no screenshots/video and run after the
build/compiler jobs finish. They retain High quality/filtering, Full lightmaps
and reflections, bloom, one actual reflection pass, 800×450 logical / **1600×900
physical drawable**, actual Immediate presentation and V-Sync off. Metal records
`display_sync_enabled:false`; phase averages of 454–576 FPS and sub-refresh loop
times establish uncapped presentation on the 120 Hz display. Each run has zero
missed ready surfaces (23,068 / 23,663 presented gameplay frames). Earlier machine
refresh plateaus remain a known measurement limitation and are not used here.

The following means are CPU wall-stage costs, not CPU utilization. Active windows
are 5.2–8.2 seconds; initial idle is 1–4.8 seconds; sleeping windows require every
bone asleep with zero active bodies, and reset samples start after 40.5 seconds.
GPU spans deduplicate timestamp IDs and include scheduling stalls, not active GPU
utilization. The single/eight cameras differ, so render/FPS columns describe
their respective workloads rather than an isolated scaling experiment.

| Workload / phase | CPU update ms | CPU render ms | Physics ms / executed 120 Hz substep | GPU pass span ms | Frames/s |
| --- | ---: | ---: | ---: | ---: | ---: |
| One / authored idle | 1.016 | 0.853 | no active solver step | 1.207 | 493.1 |
| One / active | 1.034 | 0.830 | 0.136 | 1.202 | 494.5 |
| One / sleeping | 0.769 | 0.889 | 0.027 | 1.147 | 543.9 |
| One / reset | 0.981 | 0.893 | no active solver step | 1.215 | 492.1 |
| Eight / authored idle | 0.999 | 0.933 | no active solver step | 1.253 | 479.2 |
| Eight / active | 1.448 | 0.604 | 0.799 | 1.323 | 453.7 |
| Eight / sleeping | 0.178 | 1.369 | 0.042 | 1.264 | 575.9 |
| Eight / reset | 1.028 | 0.860 | no active solver step | 1.205 | 481.4 |

Idle includes eight authored animated models; one physical actor leaves seven
animated, while eight sleeping actors stop all competing animation/controllers.
Sleeping retains exact joints/poses until reset; there is no collision/lighting
quality reduction. Single/eight peak whole-process RSS is 338.17/346.45 MiB,
including loading, not isolated ragdoll allocation. Sampled live GPU objects stay
bounded (at most 119 buffers, 31 textures and 133 bind groups after ready) across
activation/reset. Metal does not expose complete allocation-byte counters; no
total GPU-memory claim or engine-wide speedup follows from these two tiny-lab runs.

### Evidence, delivery and stage boundary

All raw runs, failed tuning probes, logs, CSVs, settings, old binaries/packages
and videos remain under `local/ragdoll-0.13.0/`, outside `target`. Final receipts
are `gates-validated.json`, `final-campaign-complete.json`, `acceptance-final.json`,
`gpu-resources-final.json`, package checks and SHA-256 manifests. The silent final
clip has **383 native frames at nominal 10 Hz / 38.3 encoded seconds**, 1600×900;
it is visual evidence and supplies no performance measurements. Ten selected
screenshots and the complete editable project export are in `deliverables/`
alongside `Places-0.13.0-Ragdoll-Cycle.mp4`. The delivery manifest lists exact bytes
and hashes. Native acceptance covers this Mac/Metal host; Windows/Linux and final
integrated CI remain unvalidated by this stage.

The current Library skill's unmodified supported helper was called **once** after
reading both prior failure logs. Its ordered 31-file batch preserved the previous
19-file request and appended the final 12 ragdoll artifacts. It exited 1 with
**`library upload failed: prepare_uploads failed`**, empty result and **no confirmed
Library IDs**. No alternate transfer route or further retry was used. Exact
request/result/log and `library-delivery-status.json` preserve the service blocker;
the local files are not claimed as delivered.

The two unrelated untracked Python audit reports retain their original SHA-256
identities. No unique evidence, shared target or previous package was removed.
No CI wait, force push, release, main merge or credential change was performed.
The final on-disk handoff records the exact staging commit, verified remote,
artifact paths, delivery blocker and explicit ownership release; final docs and
integrated validation are the next stage.

## GPU rendering and visibility pass — 2026-10-22

This is the first newly authorized sequential performance pass. Loading/memory
work follows separately. Baseline is pushed `307f9d2efcb8a6c72ca03ea4725384bfd3c53e1b`
on `0.13.0-staging`. The preserved baseline executable is SHA-256
`16c07cb27ce5452aba5a749737b511a0488c7c6ed0593d6ef70cb1c4fad4f0ec`;
the verified candidate is
`7ef8477e0eb23adfef3f284930051d6eecec80d4f9e28c3e17ffd7811ca29925`.
Inputs, executable copies, exact commands, receipts, traces, frame CSVs and media
are retained under `local/gpu-visibility-0.13.0/`, outside `target`.

### Shared change and correctness boundary

The renderer now rejects wholly hidden draw bounds using existing **opaque
architectural triangles**. Upload retains at most 512 large triangles, with
bounded temporary storage; each current camera prepares at most 24 triangle
cones in fixed storage. All of a box must lie strictly behind one actual solid
triangle and inside its three eye-to-edge planes. A coordinate-scaled margin
keeps uncertain cases visible. Triangles are never joined across openings.
This removes submitted geometry behind walls without inferring rooms or portals.

Static world, prop, current movable/door, posed character and decal bounds use
this filter after frustum acceptance. Scene and emission share the camera's
cones; planar captures prepare their own reflected view. Fully hidden reflective
bounds no longer request a planar capture. Planar mirror surfaces are excluded
as occluders because the active mirror is absent from its own capture. Cutout,
blended, moving and character geometry never supply occluders. Near-clipped,
invalid, coplanar, edge-touching and intersecting geometry survives. Simulation,
lighting preparation, light sources, transparency order and effects are unchanged.
Spotlight shadows and offline probe captures retain their original visibility;
o stale camera/pose answer or dirty-light cache was introduced.

At matched frame 180, base-scene submission changes as follows. Counts include
accounted geometry and decals/effects, excluding reflection/emission duplicates,
post-processing and UI; they establish submitted work, not GPU pixel visibility.

| View | Draws before → after | Submitted indices before → after |
| --- | ---: | ---: |
| Busy Arcade | 308 → 198 (−35.7%) | 407,502 → 313,374 (−23.1%) |
| Arcade bathroom | 28 → 28 | 20,148 → 20,148 |
| Pool closet approach | 90 → 53 | 61,944 → 28,152 |
| Stress lab / actors | 62 → 37 | 198,402 → 177,294 |
| Home | 33 → 32 | 17,160 → 16,848 |

All five native before/after images have **zero changed RGB channels**, including
Arcade at 2560×1440 and the other views at 1280×720. Thirty-five additional native
captures exercise stress-lab contacts/reset and visitor lifetime, Pool underwater/
waterline/rim transitions, Home folding-door traversal and reversal, and eight
ragdolls with pause/step/resume, Medium/High graphics, menu return and reload.
Selected captures were visually inspected. Underwater state is confirmed; door
progress runs 0→1, reverses midtravel and returns to 1; ragdoll count returns
8→0 at menu and remains zero on authored reload. These are focused native checks,
not exhaustive certification of arbitrary future maps or every lighting state.

### Matched timing and pass evidence

Twenty capture-free Release trials use an ABBA order, 240 warmup frames and
2,400 measured frames per run. Hardware is Apple M2 Pro, macOS 27.0.1 / Metal.
High quality/filtering, Full lightmaps/reflections, bloom, 60° FOV, VSync **off**
and engine limiters off remain fixed. Receipts confirm actual Immediate and the
physical drawable. Arcade uses 2560×1440; other cases use 1280×720. No build,
recording or second native run overlaps these trials. Reused collection, editable
stress-lab and shared menu assets retain their hashes; no map was rebaked.

Fixed gameplay setups are Arcade `(6.8,1.6,28; yaw25,pitch0)`, bathroom
`(-1.6,1.6,2.8;270,0)`, Pool `(13,1.6,13;90,0)` and lab
`(14.5,1.6,11;315,0)`. Menu uses its authored live camera and simulation.
Each pair below lists both trial values, without dropping the slow repeat.
GPU values are medians of deduplicated completion IDs, measuring the **union of
GPU stage timestamp spans**, including scheduling stalls, rather than active
execution/utilization. `results.json` retains means, tails, worst frames and counts.

| View | Presented frames/s before → after | Median GPU span ms before → after | Mean CPU update ms before → after |
| --- | --- | --- | --- |
| Busy Arcade | 243.77 / 244.40 → 222.59 / 249.39 | 4.491 / 4.487 → 4.399 / 4.395 | 1.036 / 1.054 → 1.000 / 1.049 |
| Bathroom | 582.33 / 581.69 → 580.00 / 581.75 | 1.104 / 1.098 → 1.101 / 1.094 | 0.955 / 0.954 → 0.941 / 0.938 |
| Pool closet | 790.76 / 726.94 → 399.42 / 724.36 | 0.850 / 0.869 → 0.960 / 0.885 | 0.112 / 0.115 → 0.131 / 0.112 |
| Stress lab | 434.62 / 434.43 → 441.83 / 442.39 | 1.461 / 1.483 → 1.448 / 1.468 | 1.711 / 1.713 → 1.702 / 1.708 |
| Live menu | 647.24 / 612.35 → 608.62 / 607.34 | 1.071 / 1.064 → 1.055 / 1.062 | 0.434 / 0.432 → 0.437 / 0.435 |

Arcade's median GPU span is about **0.092 ms / 2% lower** in these trials, alongside
substantially fewer submissions. The lab's observed rate rises about 1.8%, with
overlapping GPU spans. Bathroom is effectively flat; Pool is mixed/slower, and
menu rates are lower across the recorded repeats.
**No engine-wide FPS gain is claimed.** Rates exceed the recorded 120 Hz refresh,
but actual Immediate alone does not eliminate drawable-acquisition pacing.
Arcade's slow after repeat has loop p99 10.626 ms versus 6.371–6.787 before;
Pool's slow after repeat has p99 16.384 ms versus 3.612–3.763 before. These are
retained limitations, not renderer savings or a proven fix for the original
location-specific Pool slowdown.

Same-candidate filter-off/on diagnostics retain frustum culling and use new bounded
per-pass submitted-draw fields in raw GPU traces. Arcade submits planar **300→265**,
scene **308→200**, emission **38→35** at the sampled live frames. The deterministic
frame-180 scene count is 198 because current moving content can change bounds.
Pool samples submit scene **90→53**, emission **2→1**. In the fog/flashlight lab,
scene draws are **62→37**, emission stays **1**, and spotlight visibility stays
**29→29**: offscreen shadow casters are preserved. Bloom/resolve/UI/surface-copy
passes remain present. Arcade's captured environment has **zero fog regions and
zero global fog density**, ruling fog out as its current busy-view cause; lab
mist and Pool global fog retain their original shader and quality.

Three-sample diagnostic GPU pass spans are deliberately not treated as isolated
execution costs: Arcade planar medians are 1.208→1.256 ms, scene 4.169→4.755 ms and
emission 3.796→3.758 ms; Pool scene is 0.839→0.684 ms; lab spotlight is
0.117→0.113 ms. Metal stage intervals overlap and include stalls, so these values
cannot be added or substituted for the longer capture-free trial medians.

Bounded CPU stage traces separate acquisition from encoding. Arcade filter-off/on
reported medians are preparation **0.345→0.355 ms**, acquisition **3.524→3.474 ms**,
encoding **0.458→0.387 ms**, submission **0.152→0.138 ms**. Additional eight-second
Pool controls report preparation **0.005→0.013 ms**, acquisition **3.976→3.926 ms**,
encoding **0.263→0.218 ms**, submission **0.069→0.068 ms**. Both Pool controls contain
roughly 19–20 ms acquisition tails. These bounded traces preferentially retain
slow frames (96 maximum); they are diagnostic spans, not whole-run percentiles
or presented-FPS comparisons. The visibility preparation adds small CPU work while
reducing encoded work; independent native presentation cost remains open.

### Gates, media and sequential handoff

`cargo fmt --all --check`, strict workspace/all-target/all-feature Clippy,
`cargo test --workspace --all-features` (**257 passed**, zero failed/ignored), and
`cargo build --locked --release --workspace --all-features` all pass. The three new
triangle/box regression fixtures protect false-positive culling at openings,
near/coplanar/invalid boundaries and changed/reflected cameras or moving bounds.
They add no assets, map bake or GPU and execute below the test timer's 0.01 s
resolution. Python benchmark syntax validation passes. No hosted CI was awaited.

Eight separate six-second 1280×720 clips retain 60 Hz capture pacing and **30 FPS**
output, covering before/after Arcade, Pool, actors and live menu. They supply no
timing numbers. The inspected 27-second, 30 FPS side-by-side montage is
`local/gpu-visibility-0.13.0/montage/Places-0.13.0-GPU-visibility-before-after.mp4`;
its captions show separate measured results and explicitly retain mixed/slower
cases. Raw clips and encoder receipts remain available for the final two-pass
montage. No 2 FPS footage was used.

Only the current authorized staging branch is committed/pushed. No version bump,
tag, release, main merge, force push, map rebake, texture change, GPU-skinning,
compression, job-system or presentation rewrite is included. The unrelated Python
audit files are preserved. The final local handoff records the remote commit and
process custody. `target` is retained, with no `cargo clean`, for the parent's
subsequent loading/memory pass; all deliverable copies are already outside it.

## Large-map loading and memory pass — 2026-10-22

This is the second authorized sequential performance pass, starting at
`c1a20f0df0d6a762950d4afe1cb29974fe9416e3`. The preceding GPU draw/index reductions
and GPU timings remain that stage's results. No new codec, package revision,
streaming/job-system rewrite, GPU skinning, authored scene change, release, tag,
version bump or main merge is included here. Evidence and preserved production
executables are in `local/loading-memory-0.13.0/`, outside `target`.

### Shared loading changes

Fresh measurements reproduced the historical symptoms: Arcade reached its first
usable scene in 8.14 seconds and Lantern in 4.74/4.76 seconds. The symbolized
Arcade diagnostic located repeated whole dynamic-scene GPU uploads and PNG
identity hashing during each authored physical-prop spawn. The release stage
trace measured 5.82 seconds in physical-item installation. Authored groups now
publish one dynamic upload after their ordinary keyed spawns, material tints and
oriented poses. Individual runtime spawn/replacement behavior is retained.

Immutable runtime receiver visibility is prepared on the existing loading
worker, with its own progress phase and cancellation checkpoints. Its existing
triangle/material/alpha/light calculations are unchanged. This removes Lantern's
roughly 1.57-second visibility construction from the final upload callback.
Editor/tool installs retain an inline preparation path. Weak lifetime probes now
also observe the transport scene, without keeping an inactive map alive.

Framed, budget-checked literal and palette prop vertex runs reserve their exact
validated count. The former 4,096-slot initial reserve geometrically grew and
retained substantial unused storage. Lantern's prepared-record charge falls
**965,299,582 → 730,359,898 bytes (24.3%)**. This charge is not total process memory:
it excludes renderer visibility, other gameplay allocations and driver storage.
One essential synthetic 4,097-vertex regression exercises both encodings with
under 1 MiB, protecting this observed allocation defect without a map or GPU.

Moving visibility earlier initially increased Lantern peak RSS by about 8%,
despite the lower owned record charge. That rejected intermediate candidate is
preserved. Upload completion alone only partly reduced the overlap. The accepted
path allocates the atlas once and copies one complete page pair per step, using
nonblocking queue completion between upload families/bounded prop batches.
Lantern's former 224 MiB atlas staging burst becomes one 16 MiB pair at a time.
The complete world is published only after its uploads finish; cancellation drops
pending ownership, and completion callbacks own only a flag. Compiler/tool
callers still have an immediate complete-atlas upload entry point. Immutable
resident-atlas reuse, package hashes, bounds, fallback and cache policies remain.

### Matched native measurements

`FINAL-RESULTS.json` retains every capture-free ABBA run, executable identity,
trace and process measurement. These are fresh processes with empty application
state and **warmed disk/OS/GPU caches left untouched**, not cold-storage tests.
The host is the same M2 Pro/macOS 27.0.1/Metal system, release/all features,
High textures/filtering/lighting, Full lightmaps/reflections, bloom and FOV60.
The physical drawable is **1280×720**, VSync/caps off, actual Immediate. No FPS
or uncapped-throughput gain is claimed from these loading runs.

| Median of two matched runs | Before | After | Interpretation |
| --- | ---: | ---: | --- |
| Arcade request → first usable scene | 8.063 s | 2.456 s | 69.5% less waiting |
| Lantern request → first usable scene | 4.726 s | 4.336 s | 8.3% less waiting |
| Small lab request → first usable scene | 0.302 s | 0.339 s | 37.4 ms slower from bounded installation |
| Arcade process peak RSS | 1370.9 MiB | 1117.6 MiB | 18.5% lower |
| Lantern process peak RSS | 2105.5 MiB | 2093.9 MiB | Effectively flat; no robust peak-RAM gain claimed |
| Small lab process peak RSS | 451.9 MiB | 429.9 MiB | Lower in these runs |
| Lantern longest upload callback | 1571.8 ms | 116.5 ms | Large main-thread callback removed |
| Arcade longest loading event-pump gap | 6077.1 ms | 381.3 ms | Remaining install hitch remains explicit |
| Lantern longest loading event-pump gap | 2232.3 ms | 276.8 ms | Improved, still not hitch-free |

Worker traces separately expose mesh read/parse, shared-prop decompression and
reconstruction, prop parse/textures, lighting/collision/navigation, lightmaps,
irradiance, reflection captures and visibility preparation. Lantern still spends
roughly 0.9 seconds reconstructing props and 0.7 seconds reading/decoding its
lightmaps; no replacement storage project is hidden in this pass. The small-map
latency regression and flat Lantern process peak are acceptance tradeoffs.

### Lifetime, correctness and delivery

The final 119-second ownership campaign visits Lantern and Arcade three times
each, the small lab twice and Hub, then returns to the live menu. All ten retired
prepared-record/build/transport probes reach **zero owners** after the existing
serial cache-retirement work. At the unload seam, old prepared bytes, script
modules/VM bytes, AI agents and physics bodies are zero. The new menu legitimately
owns 105,567,345 prepared bytes, one script module and its own actors; it is not an
empty application. Oversized gameplay records remain outside the existing LRU;
the small lab's second load still exercises a cache hit. No old-map ticking or
live large-map accumulation was found, and the baseline already retired those
owners. The new transport probe extends that evidence rather than inventing a
previous leak.

Fresh-menu and returned-menu `heap` inspections report **308,859,073 and
321,398,993 live malloc bytes**. The corresponding `vmmap` snapshots show
109.3 MiB versus roughly **1 GiB of empty large-allocation regions**, and
25.3 versus 311.5 MiB of zone fragmentation. This is concrete allocator
high-water/fragmentation evidence in the present run, distinct from live map
ownership. The captured/heap-inspected transition campaign peaks at 3021.3 MiB
sampled RSS while old/new rollback resources can overlap; it is not the
capture-free fresh-load comparison. Exact attribution of the historical
**+312 MiB** observation remains unproved. Metal driver byte counters remain
unavailable; texture/view counts are not VRAM. Final menu samples have the same
60 textures, 135 views and 468 bind groups as a fresh menu; small buffer/encoder
count variations remain visible.

Five final frame-180 Arcade/Lantern/lab/Home/Pool comparisons have **zero changed
RGB channels**. Eighteen native assertions pass, including 25-body activation and
reset, the scripted visitor's 12→13→12→13 lifecycle, graphics restoration,
decode cancellation/retry, cancellation during staged GPU upload with no stale
Lantern commit, and bounded Lua allocation-failure recovery. The actual native
editor opens the existing tiny fixture, builds, plays and returns; the current
compiler exports/imports both fixture members with exact JSON/Luau recovery.
Only this explicit tiny developer fixture is prepared; no shipped map is rebaked
and no map preparation enters Cargo tests or CI.

Final formatting, strict workspace/all-target/all-feature Clippy with
`-D warnings`, **258 Rust tests** (0.50 seconds, none failed/ignored), and the
locked release workspace/all-feature build (3m 15s) pass. The one added Rust
fixture has the concrete memory-regression purpose described above. Native
acceptance is macOS/Metal only; Windows/Linux, broad hardware memory pressure and
GPU-device-loss injection were not newly certified. Hosted CI was not queried or
awaited under the standing instruction.

The existing 27-second GPU montage was saved early as
**`libfile_d745f447ea1c819196020a856099a6b1`**. The final combined montage is
**`libfile_259ae60e0de881919dd167a21a53422d`**, H.264/yuv420p, 1280×720, 30 FPS,
**41 seconds / 1,428,979 bytes / 1,230 fully decoded frames**. Its first 810
compressed frame payloads exactly preserve the prior GPU clip without
re-encoding. New clips are actual separately paced ready-scene captures; the
loading UI itself is not recorded and no smoother-loading-video or FPS claim is
made. Labels use the independent capture-free measurements and retain the small
control regression. Sampled full-resolution QA frames are readable.

`FINAL-HANDOFF.json`, exact executable hashes, native/heap traces, raw media,
intermediate candidates and the source patch preserve recovery evidence outside
`target`. The two unrelated Python audit files remain unchanged. The user's
subsequent queue instruction supersedes cleanup for this stage: **retain
`target` and editor/game executables** for the parent-owned lighting-trigger and
shared-calculation audit, followed by furniture-store work. No `cargo clean` is
performed here; ownership is released only after commit/push verification and
confirmation that no owned native/build job remains.


## Lighting-trigger and shared-calculation pass — 2026-10-22

Authorized items 1 and 3 only, on `0.13.0-staging`, from pushed baseline
`d114077008d2d108dd157b4f40ddcea2c859a302`. Earlier regional dependencies,
separated caster bounds, immutable-ray retention, sleeping ragdoll caching,
unchanged-pose fixes and water invalidation remain in use. No maps, assets,
quality settings, update intervals, solver thresholds or cache budgets changed.

### Trigger audit and shared ownership

| Trigger | Required invalidation and final disposition |
| --- | --- |
| Rigid movement, rotation and scale; doors | Exact pose bits invalidate the object's shared placement. Visibility consumes the final pose; existing regional/ray dependencies select affected lighting receivers. Repeated identical solver or door poses retain the placement. |
| Ordinary animation, tint, opacity and emission appearance | Pose revisions drive skinning. Environmental lighting depends on placement and receiver envelope, not the skin/albedo revision. Balloon and ragdoll envelope changes still refresh lighting immediately. Emission appearance does not invent a baked light source; attached lights retain their separate live path. |
| Physics, sleeping and waking | Solver/interpolated pose changes use the same placement writer. Sleep alone does not invalidate lighting. Ordinary contact wake publishes changed poses; sampling follows those current poses before drawing. |
| Light switches | Actual energy changes advance the existing lighting revision; immutable rays remain retained. No-op switches retain results. Active-source masks/direct descriptors are prepared once for each dirty receiver batch. |
| Water | Surface phase/tint/speed/strength remain independent controls. Source energy and changed caster visibility retain their existing water dependencies. Water and rigid/character receivers now consume the same final movable-visibility snapshot after carry attachments resolve. |
| Creation, suspension, removal and reset | Admission/removal and visibility changes reach the shared provider immediately. Suspended characters now skip environmental refresh, retaining their old keys until restoration; restoration samples current source, pose and visibility state before rendering. Scene reset drops placement/payload ownership. |
| Level and graphics transitions | Installed lighting/field/transport revisions still invalidate receiver payloads. Batch source data borrows the installed snapshot only until workers join; it cannot survive a transition. |

`DynamicObject` now owns one lazy matrix/center/world-bounds result between exact
pose mutations. All private pose writers use `set_placement`; physics, floats,
spins, visibility, receiver lighting and rendering share its result. It preserves
the original yaw constructor and matrix multiplication order. Appearance edits
leave this placement intact.

Rigid and character batches prepare each receiver's center/spatial refresh
plan once before dispatch. Workers consume that plan instead of repeating the
regional dependency query. Even unchanged payloads consume observed visibility
revisions, preserving conservative missed-history fallback. A lazy, immutable
batch context shares source selection, active masks and direct descriptors;
receiver-specific occlusion and the full anchor/tap calculations remain separate.
No cross-frame source cache or general telemetry framework was introduced.

The frame loop previously refreshed rigid lighting/water, animated characters,
then refreshed rigid lighting/water again after socket-attached props moved.
It now advances poses, resolves carry sockets, refreshes dynamic visibility and
rigid/water lighting once, then publishes character lighting/skinning against
that same final snapshot. This preserves same-frame correctness, including
carried props affecting other receivers. A late active-carry trace confirms
**16 dynamic passes for 8 character frames → 9 passes for 9 frames (2 → 1/frame)**.

### Measured savings and limits

A focused nine-triangle/eight-probe operation probe over 120 moving frames,
including the ordinary visibility, lighting and render-access paths, counted:

| Calculation | Before | After |
| --- | ---: | ---: |
| Matrix compositions | 11,760 | 120 |
| World-bounds calculations | 3,000 | 120 |
| Receiver dependency checks | 1,920 | 960 |
| Source selections | 2,880 | 120 |

Temporary operation counters were removed from production sources after the
comparison. Their source/fixture snapshots and raw results are preserved in
`local/lighting-sharing-0.13.0/`; instrumented fixture elapsed times are not
native performance evidence.

Native measurements used the Apple M2 Pro/Metal adapter, a physical **1280×720**
drawable (640×360 logical), High quality/textures/filtering/lighting, Full
lightmaps/reflections and bloom, V-sync OFF, resolved Immediate mode and no frame
limiter. Timing runs had no screenshots, video, full-recalculation checks or CPU
trace logging. Separate diagnostic runs recorded CPU stages and Metal acquisition
wait; GPU samples retain the existing renderer timestamp definition.

Two runs per build of the seeded carried-prop menu event reduced median CPU
update time **1.3415 → 1.0595 ms**, saving **0.2820 ms / 21.0%**. Mean update time
was **1.2532 → 1.0289 ms**. A bounded active-carry trace separately measured
combined dynamic lighting/upload work at about **0.487 → 0.453 ms/frame**.
Observed presentation remained **123.99 → 123.62 FPS**, with about **5.8–6.0 ms**
median drawable-acquisition wait in the separate carry profiles. This is a CPU
saving, not an established FPS-throughput gain.

The other initial matched blocks did not establish a general CPU/GPU/FPS gain:
Pool median update **1.0245 → 1.0493 ms**, Arcade **1.1475 → 1.2560 ms**, the
existing editor-supported stress map **2.0433 → 2.0308 ms**, and the seeded roach
menu **0.5935 → 0.6387 ms**. Presentation/GPU scheduling varied substantially
between blocks. An additional adjacent Arcade baseline/candidate control gave
**1.1625 → 1.1490 ms** CPU update, **2.399 → 2.273 ms** GPU elapsed and
**184.06 → 182.96 FPS**. Raw blocks and the control remain preserved; no general
FPS multiplier or GPU improvement is claimed. There is no new memory-leak claim
and no reinterpretation of the historical +312 MiB observation.

### Correctness, gates and handoff

- **1,282 native entity full-recalculation comparisons, zero mismatches**, across
  Pool moving/rotating props and water, stress-map animation/scripts/physics,
  carried props, menu events, ragdoll motion, sleep/contact wake/reset, light
  switches, removal, quality changes and unload/reload. These checks execute
  only on explicitly selected diagnostic captures. Existing exact water
  full-refresh regressions also pass.
- Three fixed-simulation native views, Pool/Arcade/stress map, are **pixel-identical**
  to the baseline. Native contact acceptance records 15 sleeping ragdoll bones,
  ordinary gameplay contact waking all 15, then explicit reset removing the rig.
- **34 native assertions** pass. The real native editor opens the existing tiny
  two-map fixture, Builds, Plays, returns, exports and imports; recovered JSON
  and Luau are exact. No full shipped-map bake is added to tests or CI.
- Formatting, strict `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  **260 Rust tests** (0 failed/ignored; about 0.47 s execution), and
  `cargo build --locked --release --workspace --all-features` pass. The production
  build completed in 3m23s. Two new small source-only regressions protect batch
  lifetime/lifecycle correctness and suspended-character restoration.

Only this canonical report was updated. Raw comparison data, diagnostic images,
source recovery and game/editor/compiler binaries are outside `target` under
`local/lighting-sharing-0.13.0/`; `FINAL-HANDOFF.json` records the final commit,
remote verification and ownership release. No new before/after video is useful:
the images are identical and the measured benefit is CPU work; previous Library
media remains unchanged. Preserve `target` and executables for the queued real
editor furniture-store task. That task, later polish and showcase quests were
not started. Hosted CI was not queried or awaited; no main merge, force push,
tag, release, version bump or `cargo clean` was performed.


## Furniture-store construction checkpoint — native UI access blocked

The authorized large multi-storey furniture-store stage was attempted after
`b88729e75c0d` on `0.13.0-staging`. Construction is **not complete** and the
separate polish stage must not treat this checkpoint as a finished map.

The unchanged `target/release/places-editor` was launched in the local standard
macOS bundle `local/furniture-store-0.13.0/Places Native Editor.app`. The native
editor's New folder chooser and keyboard workflow created
`projects/untitled-1792640069720/places.project.json` and its
`levels/room_1792640069720.json`. This is only the unmodified 6×6 m starter
floor/ceiling and `editor_light`, with no walls or furniture. A menu-entry toggle
changed during pointer-target diagnosis. No source JSON was generated externally.

The screen tool attached to the actual “Places - Editor” window, but mouse
activation did not correspond consistently to its displayed controls. Later
native state refreshes returned `Computer Use server error -10005:
timeoutReached`; input returned that Computer Use was inactive for this app and
required a fresh app state. Refreshing that state also timed out. The parent
reported an approval request delivered to the user at 03:04 UTC; this subthread
has no explicit approval response or denial details. That pending request was
not replaced or bypassed. This is an unresolved desktop-tool access blocker;
no claim is made that an essential editor authoring feature is missing.

`local/furniture-store-0.13.0/CHECKPOINT.json` records exact paths, progress,
errors and the proposed **After Hours — Rooms Beyond** journey: credible sales
floor/bedrooms → wardrobe gallery and required physical closet passage → crouch
service link → separate normal-sized 2×/0.5× furniture rooms → straight upper
storey stairs → squeezed low room → dense high-ceiling bed/lamp archive → final
excessive bedroom and exit. The route and all essential design requirements
remain construction work, not polish opportunities.

Save/reopen acceptance, Build/Play, export/import, native route traversal,
uncapped dense-spot timing and representative map screenshots have **not** been
performed. No map package or Library attachment is claimed. Existing assets,
engine code, unrelated audit drafts, `target` and release binaries are retained.
Resume by resolving the existing approval/access state and verifying input in
the same native editor before authoring. Do not substitute external map JSON.

## After Hours — Rooms Beyond: native construction follow-up

**Construction acceptance is complete.** The original editable project is
`projects/untitled-1792640069720`, titled **After Hours - Rooms Beyond**. All map
construction and corrections used the real native editor: keyboard navigation,
complete-pose forms, the existing model/material library, duplication, numeric
transforms and the connected-floor stair wizard. No external map generation or
source JSON import was used. Existing assets only; no engine code, model pack,
texture, version bump or later queue item was introduced.

The native connection was recovered without privacy/security changes. The
copied local app exited with status 1 because it lacked bundled assets or an
explicit asset root. Its local launcher now supplies existing repository
asset/tool paths while preserving app identity. Normal launch restored the
actual editor screen. Keyboard input works; custom-canvas pointer activation
remains unreliable. The editor retains the original saved project for polish.

There are **ten main rooms across two storeys**, counting the two early bedrooms
separately and the stair hall once; corridors and the closet passage are extra.
The saved source contains 18 floor/carrier records after the stair split,
56 wall slabs, one 18-riser flight climbing 3.5 m, **200 furniture props and
37 light fixtures**. The main route advances along the ground floor and returns
along the upper floor. It is linear, with short bedroom side visits.

| Requirement | Authored result / native acceptance |
| --- | --- |
| Good early rooms | Furnished showroom and two bedrooms: beds, paired lamps, rugs, seating, tables, cabinets, storage, plants and tabletop dressing. |
| Increasing excess | Archive and finale repeat beds, lamps, shelves and seating; selected beds are sideways, steeply tilted or partly embedded. |
| Physical closet route | Open closet-door banks lead directly into the mandatory passage; no teleport. Native traversal passed. |
| Crouch connections | 1.6 × 4 × 1.15 m service passage and 4 × 8 × 1.35 m squeezed upper bedroom; both passed crouched. |
| Interior windows | Opposed bedroom apertures look across the central corridor into the other bedroom. |
| Extreme room size | 9 m-high archive contrasts with the low, tightly furnished upper bedroom. |
| Separate scale rooms | Two 8 × 8 m normal rooms: oversized furniture (mostly 2×; shelf 1.65×) and miniature furniture (0.5×). |
| Solid traversable architecture | Existing backed floor/ceiling geometry, thick wall slabs and actual stair tool. Final route passed without jumping or a reset. |

Inventory: 35 beds, 49 floor lamps, 29 bookshelves, eight closet-door assemblies,
seven rugs, 14 sofas, 16 coffee tables, 15 cabinets, ten armchairs, eight plants,
six book stacks and three pictures. Density was preserved during acceptance.

All 200 accidentally inherited idle AI presets were removed through the native
Decorative / no AI picker; saved-source inspection confirms zero AI/nav-agent
components. The current single-selection editor has no batch behavior control.
Three doorway headers were raised 0.02 m into ceiling backing, removing all
12 duplicate-surface errors. Safety last showed 56 bounded review entries:
stair/drop edges, open thresholds/carrier perimeters and a summary of 43
model-base support contacts. No ignores were saved; this is not a zero-finding
safety claim. Native inspection also found and corrected an oversized bed/table
pinch, four tilted beds intruding into lower ceilings, an opening above the
closet tunnel and dark stair treads. Corrections were saved before final export.
The corrected route passed from the original spawn through closet/crouch,
oversized and miniature rooms, stairs (eye 1.6 → 5.1 m), low upper bedroom,
archive and finale **without jumping**. Final position was approximately
(6.709, 5.1, 19.956); the validation player then quit normally with exit 0.
Route evidence: `local/furniture-store-0.13.0/native-walk-final/route.csv`.

Native save/reopen and Build/Play succeeded. The final native **Export all levels
+ sources** is
`projects/untitled-1792640069720/after-hours-rooms-beyond.placesmap`
(**47,212,635 bytes**, SHA-256
`69604a9a2a13ec5c6579b036a64183fbbb6b8af32b58decd0465077020fc2afc`).
Compiler verification reports current=true, integrity_valid=true, no differences,
no bake required; preparer fingerprint
`c4f002badebaf160e74bffe781543b277fddd56e05b0770244e7503e43fbb0b3`.
Native **Recover editable project** opened
`local/furniture-store-0.13.0/recovered-project`: manifest, level and Luau file
were byte-identical to all three originals. The original project was restored
afterward. The native-created unused Luau template is preserved and has no
trigger/entity event link. Older intermediate packages remain local historical
evidence; use the named final package above.

Strict workspace/all-target/all-feature Clippy and the locked release
workspace/all-feature build passed with `RUSTC_WRAPPER=` because optional
sccache was blocked by the shell sandbox. Asset validation passed with zero
warnings; read-only `check-project` validates every source. No Rust tests or
engine changes were added. These checks reuse the completed build, not a CI wait.

Two capture-free native baselines used Apple M2 Pro/Metal, Full lightmaps and
reflections, bloom, high textures/filtering, **VSync off and no frame limiter**.
Logs confirm actual Immediate presentation after initial surface setup,
**1280 × 720 logical / 2560 × 1440 physical drawable**, and a 120 Hz display.
Each view has 120 warmup + 600 measured frames and zero missed ready surfaces.
Both rates exceed display refresh and have many sub-refresh frames; acquisition
and presentation scheduling still affect the tails. These are observed baselines,
not an engine speedup or a claim that macOS presentation overhead is eliminated.

| Dense view | Observed FPS | Mean / p95 loop ms | Mean CPU update ms | Draws / triangles | Whole-process peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| Archive | 240.01 | 4.167 / 8.447 | 0.089 | 133 / 119,196 | 612.7 MiB |
| Finale | 241.63 | 4.139 / 8.069 | 0.090 | 35 / 46,234 | 608.0 MiB |

Receipts, frames and logs are under
`local/furniture-store-0.13.0/bench/{archive,finale}`. Both loaded the final
package/member identity with player binary SHA-256
`5862f079a3350fb05d88f185f4c175418b19324c2de22d9b33a9ac1097627355`.
Nine separate native framebuffer capture runs were visually inspected; capture
cost is excluded from the timings. The squeezed-room image uses a fixed crouched
render eye; actual physical crouch traversal was separately completed.

The nine screenshots and final package were saved to Library after explicit user
approval. Local image copies are under `local/furniture-store-0.13.0/screenshots`.

| Library file | Confirmed Library ID |
| --- | --- |
| `after-hours-early-bedroom.png` | `libfile_504caa43481881918c37398270dd243d` |
| `after-hours-bedroom-window.png` | `libfile_5942c6d0412c8191948624ee5c46667e` |
| `after-hours-closet.png` | `libfile_f2dd414c1cfc8191a635df8f23a5ad63` |
| `after-hours-oversized.png` | `libfile_2e53ef2fe04081918bd46a3c849fbb32` |
| `after-hours-miniature.png` | `libfile_e2f283bab6888191b9b5df895af72b67` |
| `after-hours-stairs.png` | `libfile_c2083f4b802c8191a05181ae92219830` |
| `after-hours-squeezed.png` | `libfile_507711289a048191951d61dbc691009c` |
| `after-hours-archive.png` | `libfile_0de0b6c376148191ad398e99149ed8e2` |
| `after-hours-finale.png` | `libfile_54b5b9ef29a48191966df76cf76f090b` |
| `after-hours-rooms-beyond.placesmap` | `libfile_7f6439fc0fdc8191b451c07eaa87040f` |

Every Library item is version 0; exact file IDs, paths and returned metadata are
retained in `local/furniture-store-0.13.0/library-result.json`. Earlier native
editor overview: `libfile_720a24605fb481918f42596213a15317`.

The separate polish pass should refine lighting rhythm, bedroom composition,
trim and sightlines while retaining the furniture density, scale contrasts,
physical closet route and validated clearances. No known route blocker is
being handed to polish. For the later editor responsiveness pass, record the
missing batch behavior edit, unreliable custom-canvas pointer targeting and
mixed local/world height wording; do not conflate those with map geometry.
Unrelated audit drafts and prior report content are preserved, target remains
available, and no subsequent performance sweep, NPC work or menu fix started.

## Grouped performance sweep 1: shared engine work — 2026-10-22

Baseline `cf0138febe9f01969d35071b75fb989a04bc72b5`, branch
`0.13.0-staging`. Implementation and deterministic checks are complete; final
candidate native acceptance is **blocked by a pending macOS permission**, as
described below. Passes 2/3, maps, density, art, assets, quality, update intervals,
physics thresholds and script budgets are unchanged. Earlier lighting, placement,
water-dependency, loading/hash and GPU-visibility fixes were retained.

| Coverage | Disposition |
| --- | --- |
| Transforms and bounds | Existing exact-pose placement sharing remains the provider for visibility, receivers and drawing. No additional cache or repeated fix. |
| Spatial queries and raycasts | Consecutive ray samples in the same indexed cell now collect its identical candidate neighborhood once. The query still samples the same points, stamps/sorts the same candidates and preserves source order and fallback behavior. Applies to interaction occlusion and AI/script sight. |
| Script visibility, allocations and door transforms | Resolve hinged door colliders once before the actor loop, preserving the original pre-script observation boundary and hinged-only leaf set. Previously every actor rebuilt and allocated that same list. No snapshot survives the tick. |
| Idle Luau | Absent or entirely disabled VMs no longer prepare actor visibility. Any enabled module retains the full observation snapshot and original callbacks; reset/reload immediately re-enables observation. No arbitrary throttling or callback inference. |
| Idle physics and AI | Existing all-dormant solver bypass, unchanged solver-pose checks, sleeping/dormant/disabled buoyancy rejection and caught/scripted-agent perception bypass remain. Contact/flow wake checks, safety/trigger bookkeeping, clocks and enabled scripts remain necessary work. Unload still drops old script/AI/physics owners; the preceding lifetime evidence is not reinterpreted as zero total process residency. |
| Water, room/support and nearby-object queries | Pool moving props/water and dense furniture baseline profiles were inspected alongside the existing regional dependency fixes. No new measured room/water cache target justified extra invalidation or ownership. |
| Cloning, formatting and asset hashing | Door-list allocation/construction is reduced in the measured visibility path. Existing automatic-handler tokens avoid path search/allocation; model admission/hash work stays at load/admission. Remaining dominant skin/upload preparation belongs to pass 2. No speculative string/asset cache. |

Two small optimized operation probes are retained under
`local/shared-engine-pass1-0.13.0/`; neither supplies application FPS. Seven
alternating runs of 20,000 indexed rays produced identical ordered candidate
digests at every size:

| Collision boxes | Median before → after | Cell visits before → after |
| --- | ---: | ---: |
| 256 | 35.154 → 30.381 ms | 1,226,000 → 797,600 |
| 4,096 | 130.907 → 109.585 ms | 4,657,600 → 2,768,900 |
| 19,600 | 295.959 → 251.060 ms | 10,060,000 → 6,133,100 |

This is **13.6–16.3% less kernel elapsed time** and **34.9–40.6% fewer cell
visits**. The separate 16-door probe, 1,000 batches per case, preserves every
sight result: 12 actors **2.059 → 1.045 ms**, 96 actors **15.694 → 7.909 ms**,
512 actors **80.125 → 39.606 ms**. A nonempty door list now allocates once per
batch instead of once per actor. Zero actors allocate no list. These absolute
savings are small at ordinary actor counts.

Capture-free baseline native runs used the supported validation app, Apple M2
Pro/Metal, V-sync OFF, resolved **Immediate**, **2560×1440 physical / 1280×720
logical**, High textures/filtering/lighting, Full lightmaps/reflections and bloom,
with no engine frame limiter. After Hours, Pool moving props, Arcade, the existing
Luau stress map, and seeded roach/ragdoll/carry menu events are preserved in
`*-before-timing-1/`. Median CPU update was respectively **0.098, 1.1225, 1.212,
2.020, 0.666, 0.8745 and 0.813 ms**. Observed cadence ranged **167–254 FPS**;
GPU elapsed and presentation spans are recorded separately. No reciprocal CPU
time is called FPS. Separate bounded profiles retain substantial drawable wait
and stress-map skin/upload spans around 1.13–1.35 ms, a clear pass-2 follow-up.
Two diagnostic profiles exhausted the existing Luau deadline under profiling
load; the capture-free baseline runs did not. Those profiles are not timing
evidence, and no script budget was relaxed.

**Native blocker:** the final rebuilt `Places Validation.app`
(`io.github.csd113.places.validation`) requests Documents-folder access before
application initialization. macOS TCC reports that the new executable does not
match its previous code requirement and records `AUTHREQ_PROMPTING` for
`kTCCServiceSystemPolicyDocumentsFolder`. The candidate launch has produced no
scene, frames or application log. The waiting test launch was canceled and its
process exit verified. The grant remains unapproved; no application regression
or permission denial was established. Approval was requested; no permission, signing,
TCC database or security setting was changed. Candidate timing, native lifecycle
acceptance and four planned fixed-frame visual comparisons remain unperformed.
**No overall CPU/GPU/FPS gain or native visual parity is claimed.**

Final source passes formatting, strict workspace/all-target/all-feature Clippy,
**261 Rust tests** (0 failed/ignored, about 0.50 s execution), and the locked
release workspace/all-feature build (3m43s). One new asset-free 64-box regression
protects conservative ray hits, stable order, cell boundaries, reverse/vertical/
outside-grid rays and long-ray fallback. The existing one-actor script-sight
test adds opening/reset coverage; existing disabled-module, replay and callback
schedule regressions pass. No shipped-map bake or native campaign enters CI.

Raw probes, exact source snapshots, baseline/final executables, launch diagnostics
and resumable native action/capture plans are outside `target` in
`local/shared-engine-pass1-0.13.0/`. `CHECKPOINT.json`/`FINAL-HANDOFF.json` record
delivery and the remaining permission/acceptance status. No new montage is
justified before native acceptance; previous media is preserved. Unrelated audit
drafts and shared target are retained. No main merge, force push, release,
version bump, CI wait, pass-2 work or pass-3 work is included.

## Grouped performance sweep 2: render preparation — 2026-10-22

Baseline `ee24c126ce418b6cd662963c120fbdb1ec3adc9a`, branch
`0.13.0-staging`. Existing menu/carry/ragdoll, stress-map and dense-furniture
profiles were reused before editing. The selected stress traces include
**1.13–1.35 ms** character skin/upload spans; these diagnostic samples are not
all-frame timing evidence. Earlier inverse-normal reuse and exact unchanged
idle-pose detection remain baseline work.

| Coverage | Result |
| --- | --- |
| Vertex preparation | Retain each character's existing CPU target in both serial and worker paths. Initialize UVs, tangents, lightmap and fixed classification fields once; update only position, normal and live albedo/caustics albedo per changed pose. Exact position and normal arithmetic is unchanged. Remove the redundant largest-model serial scratch allocation. |
| Invisible characters | When both explicit mesh and flashlight-shadow presence are disabled, skip pose preparation, queue writes and environment/receiver preparation. Retain stale dependencies until restoration, then publish the latest pose and bounds before drawing. Animation clocks, callbacks, socket evaluation and gameplay bounds continue normally. |
| Idle, sleeping, fading and shadow-only actors | Existing settled-pose and sleeping-ragdoll revision gates remain. Shadow-only actors still prepare; opacity zero is not used as a suspension signal. Fades, playback/crossfades, ragdoll wake and lighting changes keep their existing independent updates. Camera/frustum culling is not used to stop animation or shadow/reflection work. |
| Mesh/texture/buffer lifetime | Steady-state character/static/dynamic paths retain resident resources. Visible changed poses still upload the complete original vertex format; unchanged poses upload none. Runtime roster changes still rebuild character resources at their existing generation boundary; no unmeasured resource cache or GPU rewrite was introduced. |
| Allocations, clones, formatting and hashing | Pose updates retain allocation and capacity; no new hot allocation or asset hashing. Texture identities/material construction remain in admission/upload paths. Existing source copies, event-result ownership and attached-light key work were inspected; no further measured bottleneck justified changing their contracts. |

Optimized CPU operation probes compile the source-extracted old/final uploader
kernels against the unchanged baseline animator and existing real rigs. Eight
alternating samples per case exclude setup/decode and all GPU/presentation work.
Every compared GPU vertex byte and exact pose bound agrees. Single-rig medians
are for **2,000 preparations**; crowd medians compare the old shared scratch
against separate retained targets for **120 frames**, cycling the same five rigs.

| CPU workload | Before → after | Less elapsed time |
| --- | ---: | ---: |
| Mannequin, 4,340 vertices | 159.455 → 130.829 ms | 18.0% |
| Skeleton, 9,598 vertices | 368.108 → 288.073 ms | 21.7% |
| Cockroach, 2,376 vertices | 85.784 → 65.298 ms | 23.9% |
| Sheet ghost, 3,810 vertices | 178.592 → 155.959 ms | 12.7% |
| Spooner-Man, 15,364 vertices | 785.818 → 647.649 ms | 17.6% |
| 12 mixed characters | 226.496 → 181.818 ms | 19.7% |
| 96 mixed characters | 1,841.385 → 1,496.080 ms | 18.8% |

These are **CPU kernel gains only**. No overall CPU, GPU or FPS gain is claimed.
Two asset-free six-vertex regressions protect all wire bytes/bounds across pose,
tint/fallback and singular-scale changes, allocation retention, hidden clock
progress, restoration and unchanged-pose suppression. Existing animation,
ragdoll sleep/wake, lighting and ownership regressions also pass. Formatting,
strict workspace/all-target/all-feature Clippy and **263 Rust tests** pass;
the locked release workspace/all-feature build passes in **3m44s**.
No map bake or native campaign was added to Cargo/CI.

**Native acceptance remains pending:** the rebuilt supported
`local/native-validation/Places Validation.app` (`io.github.csd113.places.validation`)
requires the user's macOS Documents-folder approval before initialization.
The previous launch was canceled before startup; approval is pending, not denied.
This pass made no native launch attempt and used only CPU tools without a window,
rendering surface or TCC change. After approval, run matched uncapped/V-sync-OFF
menu/ragdoll, stress, Pool/Arcade and After Hours timing, native lifecycle checks
and fixed-frame comparisons through that same supported launcher. Record actual
presentation mode, physical drawable and separate CPU/GPU/presentation spans.
Native image parity, candidate timings and new montage footage remain unperformed;
existing media and pass-1 acceptance debt are preserved.

Evidence, source snapshots, executables and operation probes are outside target:
`local/render-preparation-pass2-0.13.0/{summary.json,vertex-results.json,crowd-results.json,FINAL-HANDOFF.json}`.
Unrelated audit drafts, maps/assets and shared target are preserved. Pass 3 and
subsequent queue items were not started.

## Grouped performance sweep 3: editor responsiveness — 2026-10-22

This pass starts at remote-verified `6e548fb2` on `0.13.0-staging`. The existing
editor probe, bounded thumbnail implementation, preview worker and earlier editor
evidence were reused. Explicit CPU profiling opens an unchanged safe copy of
After Hours: **311 selectable objects, 200 props, 37 fixtures**, with the real
621-entry catalog. Three before/after process trials use warm OS caches; each
query/type repeats 2,000 calls. The baseline probe extracts the original
sort/filter algorithm with the same shared-slice return type as the candidate.
These are operation timings, without a window, GPU upload or map bake.

The measured repeated work was catalog sorting, lowercase search allocation and
ID copying during picker drawing, hit construction and thumbnail lookup. One
catalog-owned search index now retains the exact ID order and Unicode lowercase
search text; one result is reused only for the same picker type, theme and query.
A replacement catalog constructs a fresh browser. Visible tiles borrow their
eight-ID slice. Theme filtering, variants, keyboard selection and grid layout
retain their existing behavior; thumbnail pixels and renderer code are unchanged.

| CPU operation, median | Before | After |
| --- | ---: | ---: |
| Warm all-model list, 388 IDs | 81.49 µs | <0.01 µs |
| Warm material / fixture / entity lists | 33.12 / 17.96 / 19.30 µs | Each <0.01 µs |
| Changed model search, `chair` | 90.29 µs | 9.54 µs |
| Browser construction plus first all-model list | 287.08 µs | 266.54 µs |
| Fresh safety scan | 218.43 ms | 217.12 ms |
| Warm Inspect / Environment CPU preview | 394.80 / 394.18 ms | 389.43 / 388.61 ms |
| Transform → undo → redo → undo | 2.875 ms | 2.872 ms |

The extra index costs about **0.13 ms once**; first-use latency has no established
gain. Only the repeated picker work is an optimization result. Safety, previews,
history and cold thumbnail differences are not claimed as gains. Existing safety
results remain reused until a document change; preview requests remain coalesced
and generation-checked. Unchanged frames already request neither full operation.
Actual edits still validate and rebuild. Content/integrity hashing, asset reload,
project switching, transforms and bake-state invalidation retain their existing
paths; no timestamp shortcut or extra world/preview cache was introduced.

All **176** real-catalog type/theme/query comparisons match the uncached filter.
Before/after ordered-choice and architecture vertex/index digests match; warmed
previews retain eight image decodes rather than decoding again. Every trial
restores exact raw source through ten transform/history cycles. One four-entry,
asset-free regression protects dependency transitions, variant order, Unicode
search, allocation reuse and replacement-catalog metadata. Formatting, strict
workspace/all-target/all-feature Clippy, **264 tests** (0 failed/ignored, 0.48 s)
and the locked release workspace/all-feature build pass. An executor disconnect
interrupted the first build; after confirming no surviving build, the same gate
passed with one Cargo job in 7m19s. No quality or feature settings were reduced.

Native acceptance remains **unperformed**. The existing
`local/furniture-store-0.13.0/Places Native Editor.app`
(`io.github.csd113.places.native-editor`) stalled before initialization in a
protected-file `__open`; TCC records an AllFiles preflight without a grant.
This establishes unresolved access, not a confirmed separate Documents prompt or
user denial. Its waiting launch was canceled, the original launcher restored and
no process remains. The inherited Places Validation.app Documents approval is
still pending. No security settings, signing or alternate native identity changed.
Native cold/warm UI latency, selection/history visuals, project switching,
reload/preview lifecycle and bake-panel acceptance, FPS and new montage footage
remain unperformed. Original media and both earlier passes' native debt remain.

Evidence, source snapshots, direct binaries and raw trials are outside target in
`local/editor-responsiveness-pass3-0.13.0/`, especially `summary.json`,
`source-custody.json`, `before-*.json`, `after-*.json`, native access receipts and
`FINAL-HANDOFF.json`. Original project/export, safe copy, launcher and unrelated
audit drafts are unchanged. The cumbersome inherited per-prop idle-AI clearing
remains a separate authoring UX issue. No later queue item was started; no CI wait,
main merge, release/version bump, force push or cargo clean occurred.

## Collection destination quests (0.13.0)

Implemented on `0.13.0-staging`, starting from remote-verified `50c74238`.
The editable authority remains `projects/environments`; the deliverable is
`assets/levels/environments.placesmap`. This task does not edit the furniture
project, the menu overlay, or the later polish/cleanup work. **Native acceptance
remains pending**, as described below; CPU simulation is not a native walkthrough.

| Destination / character | Favor and visible result | Recovery and admission |
| --- | --- | --- |
| Home / caretaker | Return the actual checked cushion. The existing hand attachment, corridor walk and whole-assembly hidden retirement remain; completion follows retirement confirmation. | Wrong held items are rejected. Hall reset/reentry restores an unfinished attempt. Completed visits acknowledge the remembered favor. |
| Pool / attendant | Launch the yellow duck in the second pool. Four continuous seconds of released, upright flotation away from the banks wins the regatta; the duck remains physically afloat and the attendant celebrates. | Requires the exact physical duck, water-height band and upright pose. Holding it or landing on a dry table cannot count. Existing water-demo restore recovers it. |
| Office / night archivist | Drop three loose desk pages into the open waste crate at upstairs reception. The pages remain visibly in its real open interior; the archivist acknowledges the completed cleanup. | Exact identities, released bodies and actual solver sleep below the rim are required. A fitted bottom and four side collision slabs support the crate. Its restore button resets all three pages and the local count; mugs/books cannot substitute. |
| Cosmic Lanes / lane attendant | Start with ten upright pins, launch the actual blue ball and knock down all ten lane-one pins. Fallen orientation is read from physical bodies; the attendant celebrates with a bell. | No completion from a reset switch or fallen pins alone. Held pins and crossing the foul line stop scoring. Existing lane reset remains and re-arms the challenge through the attendant. Multiple honest throws are allowed. |
| Lantern Hollow / campfire circle and Jun | Six friends occupy seven original stumps. Find Jun behind the conifer northeast of the fire; Jun walks the viable route back, faces the fire and backs into the empty seat through a sit clip. Completion waits for the settled seated state. | No teleport or alternate lantern puzzle. Reentry/reset restarts an unfinished search. A completed revisit remembers the favor and Jun walks back again; only one friend and one completion exist. Clear physical obstructions to let the same route resume. |
| Winter / ice coach | Trace the near-left, far-right, far-left lantern course on the frozen pond, then return to the coach. The coach recognizes the completed figure. | Wrong-order proximity does not advance. Leaving the ice during the figure discards the attempt; the coach restarts it. No timer, inventory or light-toggle completion. |
| Beach / shade keeper and crab | Speak to the sun-stranded crab and accompany its actual walk to the striped parasol. It stops if crowded or left behind, and settles visibly in the shade. | Keep one to six metres away. The fitted 0.43 m navigation radius clears the palm and lounge chairs. No pickup or cross-level inventory; reentry restores the local scene while retaining completion. |
| Frutiger Aero / glass gardener | Balance the blue/white/lime air handles (1/2/4 measures) to exactly five, then speak to the gardener. Mist, an authored water emitter and one animated fish restore the fountain scene. | All handles on gives seven and cannot win. A wrong setting remains freely reversible. The single-live spawn group prevents repeated fish rewards; completed revisits restore the payoff once. |

F1 supplies local discovery/help; dialogue remains dismissible through the shared
message UI. Original physics, weather, lights, routes, ambient interactions,
bowling reset and destination return doors are retained. Added stationary NPCs
use animation/interaction without idle AI crowds. Only Jun and the quest crab add
navigation bodies; Home retains its original caretaker body. Quests do not issue
per-frame light updates or rebake lighting. New physical observation scratch is
retained/reused only when enabled scripts can read it, preserving absent-VM work.

### Completion and the room

`api.completed(destination)` reads a bounded package-session set;
`api.complete()` can mark only the running level. Publication is transactional:
a failed Luau call publishes neither completion nor its queued actions. The
set admits at most 64 printable destination identities; repeated completion is
idempotent. Fresh map VMs, local resets and graphics changes retain the attached
set. The player saves changed completion to a small atomically replaced JSON in
its ordinary `saves/` directory, keyed by SHA-256 of the installed package path.
Rebuilding that package at the same path preserves progress; a different package
path is isolated. Reads are capped at 16 KiB and validate identities/counts.
A failed write retains session progress and logs the failure; a real level
boundary retries it. No script filesystem or arbitrary persistent state API was
introduced. New scripted exports negotiate `campaign-progress-v1` explicitly.

On the next hub load, the **actual eight destination IDs** must all be complete.
The authored `kindness_door` is physically locked and closed by default; seven
completions cannot open it. All eight unlock and open it, announce the return,
and start the host's welcome scene. The Room of Small Kindnesses is a real
10 × 8 m extension at x=24..34, z=3..11, height 3.8 m, through one opening in the
existing east wall. It has non-overlapping floors, four keepsake tables, eight
inspectable mementos, two chairs, a plant, fixed warm fixtures and a keeper who
connects the eight favors in dialogue. The ledger beside the door reports the
remembered count. Objects remain local; no destination item is transported to
unlock the room.

The original skeleton has only three held poses. Jun therefore uses the separate
`collection:skeleton_friend` derivative, authored by
`tools/props/build_collection_skeleton.py`. Its source SHA is guarded; the
original BIN prefix, nodes, skin, geometry, UVs, materials, PNG and all three
pose clips are preserved. Only walk, sit-down and seated continuation data are
appended. The existing 4,650-triangle/256-pixel reviewed geometry allowance is
pinned to the derivative's exact hash in `completed-models.json`; no global
budget is raised. No new texture is generated or resized. `--check` reproduces
the saved GLB, and explicit loop/sit-endpoint checks verify continuity. Native
motion/contact appearance is still an acceptance debt.

### Verification and remaining acceptance

Focused Rust fixtures exercise the real quest modules without assets or map
baking: released-versus-held flotation, exact and settled recycling, a witnessed
blue-ball launch versus unearned fallen pins, ice order/bank recovery/coach
return, wrong Home hand-ins and retirement timing, exact Aero pressure plus the
real reward spawn, transactional rollback, forward/reverse completion orders,
fresh VMs, resets, repeats, and the real hub script's gate after every step.
One small disk fixture adds unique coverage for save round-trip/replacement,
idempotence and malformed/oversized admission. These checks use a few dozen
bounded ticks and one tiny temporary JSON, rather than full-map tests. The first
full workspace run overlapped the heavy bake: 269 passed and one script reached
its cooperative wall deadline. The idle final run passed all **270 tests**, zero failed/ignored, in 0.49 s.
No VM deadline, warning level or test ignore was added to accommodate that run.

The explicit `collection-quest-check` developer binary independently bakes only
navigation and simulates the **actual edited** Home, Hollow and Beach collision
worlds with live door rules. It is never run by Cargo tests or ordinary CI and
does not bake lighting. Home reaches the existing retirement corridor; Jun
walks and the authored seated state completes; the crab reaches shade and
completes. This check caught an initial crab/palm overlap; the start, route and
body envelope were corrected before delivery. A stationary-character clearance
review also moved the archivist out of an upstairs partition into the clear
reception aisle before final export. Final authoring review gives the new
reward furniture explicit standable collision dimensions, seats the course
lanterns relative to the real ice floor and fits the ledger flush to the east
wall using its measured back plane. The resumed export includes those final corrections; the small hub was
rebuilt and the completed Home, Pool and Office bakes were reused. It does not prove Home's rendered
socket contact or opaque occlusion guard, which still require the native player.

Source/handler/schema validation and catalog validation pass with zero catalog
warnings. Prop checks, existing entity clip checks, derivative byte/rig/texture
preservation and derivative loop/sit continuation checks pass. The texture check
passes with its 96 existing generator-manifest warnings. The hub's production
geometry check reports **zero errors** and 13 warnings for the existing open
display-bay joins; there is no reward-room finding and no new suppression.
Formatting, strict Clippy, final workspace tests/build, exported integrity,
current fingerprints and byte-identical editable recovery receipts are recorded
in `local/collection-quests-0.13.0/FINAL-HANDOFF.json`.

The sandbox initially exposed no Metal adapter for offline reflection capture.
Host graphics access permitted the ordinary compiler capture path; quality,
reflection probes, lightmap density and prepared variants were not reduced.
A temporary export was canceled before publication when the final compiler
identity changed. Final export uses frozen sources and normal provenance checks;
no fingerprint was patched by hand. A later Mac-offline event ended that
exporter during Arcade preparation. Process absence was confirmed before any
resume. The published package and original partial directory were preserved.
Untouched manifests and SHA-verified spool records restored a private cache;
native verification proved Home, Pool and Office current/integrity-valid. Hub
retained its original records and the planned small collision repair. The retained
Arcade ZIP was an old seed, so its interrupted, unpublished lighting was not
claimed as recovered. A detached native export resumes from the verified cache,
reusing completed current members and building unfinished members. No manifest
fingerprint or prepared-light record was altered to claim a cache hit.
Final export exited 0. The verified package is **460,016,429 bytes**, SHA-256
`d4b720092f9842395aab23eeb3c1e8b416b890fe6f6f25b961d25eb9e3be7d7d`.
Native integrity validation passes, and `verify --require-current` passes for all
nine members. Every member retains a failure-free Full/full preparation with
lightmaps and its authored reflection-probe sets. Import recovers all **20
declared editable files byte-identically**: manifest, nine levels, nine Luau
modules and thumbnail. The ancillary repository README is outside the existing
declared-file snapshot contract. Receipts retain the verified fingerprints,
variant metadata, source hashes and original-package backup. Generated
`.placesmap` outputs retain the repository's existing ignored-file policy; source
and the new model are tracked.

**Native blocker, inherited and still pending:** interaction walkthroughs,
Home held-item/hand attachment timing, Jun's walking/seating appearance, door
clearance, reward-room appearance, all-eight native hub return and new screenshots
have not been accepted.
`local/native-validation/Places Validation.app`
(`io.github.csd113.places.validation`) needs the user's macOS Documents approval;
its previous prompt was canceled before initialization, not denied. The original
`local/furniture-store-0.13.0/Places Native Editor.app`
(`io.github.csd113.places.native-editor`) also retains unresolved protected-file
preflight; no separate denial or new prompt is established. The user must approve
access for the existing app before these routes can run. No app identity,
signing, protected-data location, permission database or security setting was
changed. No ChatGPT UI was used. This task does not discharge passes 1–3's native
acceptance debt, claim the entire queue validated, or make an FPS claim.

Only the authorized staging branch is committed/pushed; the final handoff records
HEAD and its verified remote ref. The two unrelated Python audit drafts remain
untouched. No CI wait, main merge, force push, release/version bump or target
cleanup is part of this task. Menu edge work, furniture polish second-last and
final verification/cleanup remain subsequent work.

## Main-menu left-edge overlay correction

The dark title/navigation backdrop began at x=0 of the centred 480×272 UI
region, rather than the physical drawable. Even 1280×720 leaves a four-pixel
left margin; wider aspect ratios expose more background. Moving geometry left
alone could not fix it because the UI projection clipped at that inner region.
The UI pass now covers the drawable using an expanded orthographic projection
that preserves the existing content scale and origin. The menu fills the left
margin at its original 0.74 opacity and extends its backdrop through vertical
margins, retaining the original x=0..224 fade, title, selections, colors and text.
Both the projection and cached backdrop update when physical drawable size
changes, including Retina scaling and window/fullscreen transitions.

One asset-free regression checks the actual menu geometry, resize cache and
renderer projection across all five offered resolutions, reference, ultrawide,
portrait, odd-sized and Retina drawables. It also checks unchanged content,
left-edge opacity, full-height coverage, zero-size safety and logical-pointer
alignment in both window modes. It uses small vertex lists, no GPU, assets or map
baking; these code checks do not establish native visual acceptance.

Formatting, strict workspace/all-target/all-feature Clippy, **271 workspace tests**
(zero failed/ignored, 0.48 s) and
`cargo build --locked --release --workspace --all-features` pass. The release
build finished in 7m32s with one Cargo job. `RUSTC_WRAPPER` was unset because the
sandbox could not start the configured `sccache` wrapper. No lint was suppressed.

Native acceptance and screenshots remain **pending**, not denied: the existing
`local/native-validation/Places Validation.app`
(`io.github.csd113.places.validation`) still requires macOS Documents approval;
its previous prompt was canceled before initialization. No new launch or
permission bypass was attempted, and no screenshot Library ID was created.
Existing projects, map packages and unrelated audit drafts are untouched. No
map was rebaked; the renderer source participates in the compiler preparation
identity, so subsequent package-currentness checks must use the new build.

## Furniture-store map polish — 2026-10-22

Completed native authoring and native player acceptance for **After Hours - Rooms
Beyond**, preserving the ten main spaces and two-storey linear route. All rooms,
walls, stairs and original 200 props remain. Twenty-four early-bedroom transforms
now compose bedside storage, separate sideboards, facing seating, reading corners,
lamps and wall art. Four archive beds have less regular yaw. Four existing closet
assemblies were duplicated to deep side walls: **204 props, 37 fixtures, 315 editor
objects**. Fourteen fixtures distinguish warm bedrooms, a cool tall archive, a
subdued squeezed room and a green-tinted finale. No furniture was removed, no AI
components were introduced, and no model or texture assets were added. Existing
tilted/embedded beds, oversized/miniature rooms, bedroom windows, mandatory closet
passage and low connections remain. The spawn position is unchanged; its facing
is now 180 degrees toward the journey. Crosshair is explicitly On, matching its
previous default.

Every map change was authored through the real native editor's complete-pose,
Configure, duplicate, Map Settings and Save controls. Map Settings exposed a real
input defect: `Popup::ComponentText` was absent from `update_window`'s native
text-input activation match. Cmd+A cleared its input, but typed/pasted digits
produced no characters. The five-line repair activates the existing SDL text
input for that existing modal. Native reproduction after rebuilding accepted
`180`, applied it, saved it and exported it. No map JSON was generated externally.

Formatting, strict workspace/all-target/all-feature Clippy (17.61 s), 23 existing
editor tests (0 failed/ignored; 0.16 s) and the locked release workspace/all-feature
build (7m47s) pass. No new tests or lint suppressions were added. Asset validation
passes: 622 assets, zero warnings. Project schema/source validation passes.
Geometry review reports zero errors and 44 pre-existing warnings at open
connections, stair carriers and unused carrier margins; these were retained as
findings, not suppressed or represented as a warning-free shell.

Native Project export completed with the current compiler, including Static and
Full lighting. `verify --require-current` passes with valid integrity, no source
or preparation differences, and no required bake. The new package is
`projects/untitled-1792640069720/after-hours-rooms-beyond-polished.placesmap`
(47,347,610 bytes; SHA-256
`261e52e5928e0065d819bd80281cc8649dd16482c7ab5c6611eb8dcbedb68537`).
The preserved construction package remains unchanged. Native **Recover editable
project** opened the final export with 315 objects; its manifest, level and Luau
script are byte-identical to the saved source. Recovery lives at
`local/furniture-polish-0.13.0/recovered-project/`.

A direct native Full/high player walk used ordinary walk, turn and crouch inputs
from the saved spawn, through the closet's 1.15 m connection, oversized/miniature
rooms, staircase, upper low room and archive to the finale. It used no jump,
noclip, teleport or pose-setting action. The eye dropped to 0.8 m in the closet,
rose to 5.1 m after the stairs and dropped to 4.3 m in the upper low room, ending
at (6.95, 5.10, 19.95). All 2,399 ready frames presented, zero missed. Native
captures and player/physics logs accompany the walk; no route trap was observed.
Nine final-package fixed-view captures also passed and were visually reviewed,
including the early furnished bedrooms, interior window, closet, tall archive,
finale, added side-wall closets and corrected spawn view. Representative PNGs:
`local/furniture-polish-0.13.0/captures/early-bedroom-final/view.png`,
`captures/archive-final/view.png`, `captures/finale-final/view.png` beneath that
same evidence directory. All captures are separate from timings and outside
`target`; no new Library upload was performed.

Two capture-free dense-view measurements used Full lightmaps, High textures and
filtering, Full reflections, bloom On, VSync Off and observed Immediate mode,
with matching 1280×720 logical / **2560×1440 physical** drawable. Each completed
120 warmup plus 600 measured frames with zero misses. Archive: 119.92 observed
FPS, mean loop 8.339 ms, p95 11.587 ms. Finale: 119.96 observed FPS, mean loop
8.336 ms, p95 10.588 ms. Both plateau near the display's 120 Hz pacing despite
Immediate and no requested limiter, so **uncapped throughput and speedup are
inconclusive**. Density was retained; no performance gain is claimed.

Successful native validation uses current `target/release/places` directly via
`tools/bench/frame_time.py`, explicit repository asset root and isolated state,
as required by `tools/bench/README.md`. It initialized, rendered and exited
normally without an observed Documents prompt. The historical Validation.app
Documents request was neither used, relaunched nor resolved. Existing editor
bundle `io.github.csd113.places.native-editor` and its unchanged `Contents/MacOS/
launch` wrapper worked for all polish authoring/export/recovery without identity,
signing, privacy or protected-data relocation changes. A save/quit/reopen before
the final facing edit passed. A later cold launch after final recovery stalled
before initialization in `__private_getcwd > __getcwd > open$NOCANCEL`; CUA screen
binding timed out. Those task-owned idle launches were cancelled, with no unsaved
edits loaded. No denial or visible permission prompt was confirmed; its permission
outcome is unknown. Do not infer a pending approval or retry a bundle gate from
these successful direct player results. Final editable recovery and direct player
acceptance are complete; the cold editor launch remains a separate access issue.

Receipts, hashes, route controls, original backups and findings are in
`local/furniture-polish-0.13.0/acceptance-final.json` and `CHECKPOINT.json`.
The scoped changes leave unrelated audit drafts and target intact. No CI wait,
main merge, release, version bump or final cleanup was performed.

## Final verification and cleanup — 2026-10-22

This final queue item starts from remote-verified `660a8e64892d67769e2623ddf010c97e10eca5b0` on `0.13.0-staging`. The scoped implementation commits remain `ee24c126` (shared queries/inactive scripts), `6e548fb2` (render preparation), `50c74238` (editor picker), `4ca5aedf` (quests), `8ba9204e` (menu backdrop), and `660a8e64` (furniture polish/native text input). Earlier pending-launch statements describe those earlier attempts; the current supported access results below supersede their required native acceptance where exercised. Windows/Linux remain unverified.

**Final macOS acceptance and cleanup are complete within the limits below.** Native gameplay, current full-quality packages, exact editable recovery, focused lifecycle/visual checks, required quality gates, pacing investigation, additional montage delivery and post-clean native launch pass. Evidence is retained in `local/final-verification-0.13.0/`; `FINAL-HANDOFF.json` records exact commit/remote identities and released ownership. Windows/Linux and uncapped presented throughput remain unverified, so this is not full-version validation or a release.

### Native gameplay and persistent reward

All eight distinct quests have now completed through the real native player, ordinary movement, pickup and interaction controls. Actual completion saves are carried byte-for-byte between isolated sessions; no completion flags are generated or inserted. The package path stays canonical, so the same real save identity is retained. The final eight-entry save and the later real Arcade-to-Hub return are preserved under `quest-arcade-3/saves/` and `eight-return-native/`.

| Favor | Native acceptance evidence |
| --- | --- |
| Home | `quest-home-1`: actual cushion pickup, NPC handoff before thanks, repeated interaction, whole-assembly opaque occlusion before retirement, hidden item and completion save. `home-contact-visible` adds visually reviewed post-dialogue palm support and carried-item identity/socket traces. `home-revisit-persisted` retains all eight receipts and returns the completed dialogue repeatedly. |
| Pool | `quest-pool-1`: attendant interaction, real table pickup, normal carry through the room, released duck in water, four-second upright flotation and saved completion. |
| Office | `quest-office-full-2`: three real pickups and carried returns; all three pages sleep in the open crate at approximately Y=7.237/7.250/7.263 m before completion. The first route lost its third page at a constrained corner; the accepted route pauses at turns and preserves the actual item. |
| Arcade | `quest-arcade-3`: attendant task, held blue ball, release from behind Z=20 foul line, ten real pins physically below the authored upright threshold, then saved completion. Earlier shorter observations found six to eight fallen pins; they were not accepted as completion. |
| Lantern Hollow | `quest-jun-2`: ordinary forest route planned around authored trunks/rocks, finding and speaking to Jun, live navigation, then the real `quest_seated` clip at the empty campfire stool. Completion follows the seated state. No actor or player pose is set. `jun-revisit-persisted` restores the actual seated clip on a later visit, with a clear native capture after dialogue dismissal. |
| Winter | `quest-winter-3`: all three ice marks in order, real slippery movement/braking, the three-mark acknowledgement, then return and speak to the coach. Earlier overshooting routes remain failed attempts. |
| Beach | `quest-beach-2`: keeper and crab interactions, ordinary route around the stilt house, companion distance maintained, actual slow crab navigation to the parasol and saved completion. |
| Aero | `quest-aero-3-old-bound`: gardener interaction, physical blue/lime valve controls yielding pressure five, fish/fountain reward, and real return to Hub. Final torso-aim repair acceptance is tracked separately from this preserved-package run. |

`hub-early-2` reaches the eastern door around the benches with a fresh save: the ledger says 0/8, the door is locked/closed, and normal movement stops at its physical leaf. `eight-return-native` starts in a later Arcade session using the real eight-entry save, interacts with the return portal, receives the eight-favor welcome, observes the door unlock and open to 90 degrees, walks through into the furnished room, talks to the host and inspects both keepsake tables. Native screenshots place the player inside at approximately (31.32, 1.60, 5.68). This is the actual room and door, not a replacement scene or overlay.

### Interaction-bound correction and package custody

Native torso aiming exposed a concrete authoring defect in the new NPC placements: omitted `size` selected the documented `[0.6, 0.9, 0.6]` interaction box. Standing characters rendered about 1.72 m tall, so ordinary torso aim missed. Following `MAP_AUTHORING_GUIDE.md` sections 19 and 29, fourteen new NPC placements across eight showcase levels now author model-sized interaction boxes: seven standing mannequins, Jun, and the six campfire friends. Jun additionally uses local depth **1.9 m** to cover the seated animation's approximately 0.75 m backward root displacement. They remain non-solid; rendered meshes, placements, art and quest rules are unchanged. The initial failed aim runs and lower-aim diagnoses are retained. A normal self-contained Static fixture established the seated correction; its unique document ids and removed Hub transition are diagnostic setup only. The final production full-quality package then passes visible seated body targeting, live walk-to-seat restoration, ordinary Jun dialogue and repeat dialogue with the actual eight-entry save. `final-jun-seated/visible-jun-target.png` was visually reviewed with the interaction prompt present on Jun's body. Commit **`536ee0faf7ed5f54524b1a1ce4f58db866dea85b`** contains only these eight map-source fixes.

The polished furniture package remains current and integrity-valid, with accepted hash `261e52e5928e0065d819bd80281cc8649dd16482c7ab5c6611eb8dcbedb68537`. Its full native route, exact recovery and nine captures are reused; both original and polished packages remain. The showcase initially required a normal full-quality export for the changed preparation identity. That completed at 460,017,005 bytes, hash `dffee4b28e0c105cc9e486eb81e831a48e965cd1a79c1ac1039348ace3dae549`, passing all-member currentness/integrity, exact 20-file recovery, native torso/repeat callbacks, all six campfire friends and the ordinary eight-completion room return. The subsequent seated-Jun correction uses the normal exporter to rebuild only Lantern Hollow: **eight verified member record sets are reused byte-exact**, with no unrelated rebake. No fingerprint is patched and full-quality lighting is retained for all nine members.

The **final published showcase** is `assets/levels/environments.placesmap`, **460,017,013 bytes**, SHA-256 **`f94bcc5214967b10ba71e471d4e2d094f06f20d155d7f33ac1a62207f79118c0`**. It passes all nine `--require-current` and integrity checks, intact dependencies with no validation warnings, and exact recovery of all **20 declared editable files** into `local/final-verification-0.13.0/recovered-final-editable-project/`. `final-package-verification-jun.json` and `published-final-package.json` retain the results. `final-source-scope.json` proves the only semantic differences from the original accepted gameplay package are fourteen authored NPC size additions; all nine scripts, the manifest and every other map field are unchanged. `final-published-eight-return/` repeats the ordinary Arcade portal → Hub → real furnished room route on these final bytes; its room capture was visually reviewed. Original published bytes remain in `environments-before-final-verification.placesmap`. Generated packages are ignored local deliverables; their editable sources and this report are committed.

### Native menu, editor and lifecycle verification

The current native menu was captured through live resize at actual drawables 1600×1200, 2560×1440, 2200×900, 800×1400 and 1602×902, plus native fullscreen at 3024×1898. Its pre-fix preserved player was captured at the same five window sizes. The bright title/navigation pixel masks match exactly: zero changed mask pixels in every pair. Native captures show the backdrop reaches the physical left edge and vertical margins; actual window state/focus is recorded. These are visual checks, not FPS measurements. See `menu-native-comparison.json`, `menu-current-resize/`, `menu-before-resize/` and `menu-fullscreen/`.

The unchanged existing native-editor launcher now initializes successfully without an observed permission prompt. Current browser selection, cold/warm grid, theme, search and no-match pass on the 315-object polished project. The preserved pass-3 baseline also passes through the same existing editor executable path and unchanged launcher; exact current executable bytes are restored afterward. The two native grid images have **zero changed RGBA channels**, identical documents and eight preview image decodes retained across the warm interval. Existing app identity, signing and permissions are unchanged. Native timings collected during the export are functional diagnostics, not accepted latency or FPS comparisons; the prior isolated CPU probes remain the scoped performance evidence.

A safe project copy inside the repository passes native selection, dirty transform/preview, undo, redo and final undo. The restored in-memory document equals the initial document exactly, dirty is false, history stays bounded, and preview decodes remain eight. `editor-history-summary.json` and `editor-native-parity.json` retain these observations. The earlier LaunchServices/getcwd stall is not retrospectively called a denial; the successful current supported route is recorded as its own result. Historical `Places Validation.app` Documents approval remains untouched and was not used. Subsequent CUA binding succeeds on the existing native-editor identity. The actual macOS Open-folder dialog opens the safe furniture project and a safe Relay Annex copy in both directions; the reopened furniture document exactly equals its initial in-memory document, with clean source files. `editor-native-open-summary.json` records both successful bounded launches. A three-level switch cycle returns to the original document with clean sources, and its read-only bake panel correctly labels the older fixture preparation-stale. No unrelated level is baked. `editor-switch-summary.json` retains the cycle and panel capture. The separate current furniture package check passes current → unsaved-edit unknown → undo → current, with exact document restoration and eight unchanged image decodes (`editor-bake-state-summary.json`).

The existing focused eight-ragdoll lifecycle recipe passes on the current player. Physics pause and menu pause preserve exact poses and fixed-step count; explicit single-step advances 31→32. Medium/High graphics refresh retains eight ragdolls and 112 joints. Actual return to the live menu releases all eight ragdolls, joints and 312 pose-cache entries; fresh reload returns the authored world with zero ragdolls/joints/pose caches. The carry recipe verifies socket ownership, release on ragdoll activation and return to animation on reset. These checks do not assert that total process/GPU residency returns to its original baseline.

The existing compact performance-lab recipe also passes: contact activation wakes the 25-body group, reset restores all 25 dormant bodies, `visitor_point#1` appears, expires with its mesh removed, then respawns once. Script diagnostics are empty throughout. These explicit native fixtures do not enter Cargo tests or automatic CI, and no unrelated map is baked.

### Performance and media reconciliation

Earlier isolated CPU probe results retain their exact scope: shared ray/door query work and ordered-output parity, retained character vertex preparation with exact vertex-byte/bounds parity, and unchanged editor list reuse. The mixed 12-character preparation probe takes 226.50→181.82 ms for 120 updates (19.7% less CPU kernel work). The cached unchanged model-list probe takes approximately 81.5→under 0.01 microseconds per call. These measurements are not FPS gains and do not establish native editor input-latency gains.

The final matched native series uses the original furniture package, the pre-quest Pool/Arcade package, and the existing performance-lab package, with the same coherent historical catalog and unchanged referenced asset bytes in both builds. Runtime containment validation remains enabled: scratch assets are real contained directories/regular files, not escaping symlinks. The two rejected setup attempts remain recorded and excluded; one old package rejected today’s expanded catalog and another correctly rejected the symlink fixture. Neither fallback data nor the failed launch is counted as a result.

Sixteen accepted capture-free runs alternate before/after then after/before: four scenes, two repeats per build, 120 warmup and 360 measured frames each. All use High textures/filtering, Full lightmaps/reflections, bloom On, VSync Off, no recorder or application limiter, actual Immediate and 1280×720 logical / **2560×1440 drawable**. Every run presents all 480 ready frames with zero misses and identical median submitted draws/triangles within its pair. The owned exporter is SIGSTOP only for the measured interval and is confirmed SIGCONT afterward; its retained RSS is recorded. No other owned build, native player or recorder runs during measurement.

| Matched scene | Before observed FPS range | Current observed FPS range | Before/current mean CPU update, ms (two-run average) |
| --- | --- | --- | --- |
| Original furniture archive | 120.00–120.12 | 119.81–120.00 | 0.216 / 0.227 |
| Pool | 119.985–119.986 | 119.980–120.021 | 0.285 / 0.294 |
| Arcade | 119.93–119.97 | 119.99–120.11 | 1.328 / 1.278 |
| Performance lab | 119.65–120.04 | 119.65–119.66 | 2.414 / 2.353 |

These descriptive CPU differences are small and are not presented as an application-wide gain. GPU pass-span means vary with queue scheduling and presentation; they are neither active GPU utilization nor additive per-pass costs. The first series did not explicitly activate native focus, so the Lab before/current and three pacing controls were repeated with an explicit focus action. The separate resource trace confirms true native input focus throughout the measured profile, actual Immediate, and Metal `display_sync_enabled=false`.

The focused ordinary Lab runs remain near 120 FPS. With scene rendering disabled, the native surface still cycles at **120.06 Hz**, with approximately **0.002 ms** rendering and **5.346 ms** swap/acquisition. Discarding acquired frames instead of presenting runs approximately **215.7 iterations/s**, explicitly not presented FPS. In the focused presented profile, selected render-stage medians are 5.565 ms total, **5.152 ms drawable acquisition**, 0.219 ms encoding and 0.144 ms submission. The SDL display independently reports 120 Hz. Successful frames never enter the application’s unavailable-surface yield. These controls support presentation pacing as the dominant measurement limit on this host; no OS/security setting or speculative presentation change is made. **Uncapped presented throughput gains remain inconclusive.** See `timing-final.json`, `timing3-campaign.json` and `timing-focus-campaign.json` for full conditions, identities, samples and limits.

Four separate native fixed-frame pairs use the same frozen inputs and 2560×1440 drawable. Furniture, Pool, Arcade and the performance lab each compare with **zero changed RGBA channels**, retaining the same native scene submissions. This supplements the isolated ordered-query and GPU-vertex parity evidence without claiming unchanged animation from unmatched wall-clock footage.

The additional montage is `local/final-verification-0.13.0/montage/Places-0.13.0-final-verification.mp4`: **17 seconds**, 1280×720, 30 fps H.264/YUV420P, fast-start, **773,818 bytes**, SHA-256 `52b9b57e235e811346bc372788be01b530bc9382fc635a472c2369bbce4e2781`. Both actual six-second native lab sources contain 180 frames, loaded the exact same Full member, camera, settings and assets, and come from the preserved pre-pass-1 and current executables. Their explicit recorder pacing is excluded from all timings. Editor parity, Jun’s later-visit seat and the furnished hub are explicitly labeled native stills. The montage labels CPU kernel savings and the unresolved 120-Hz throughput ceiling; it invents no missing footage. Three representative frames were visually reviewed for text/layout and correct source imagery.

The montage is saved to Library as **`libfile_08ff485a78408191b8d75b3378f138ad`**, file **`file_00000000c51881f48fadced181b8d816`**; its local identity/version metadata is persisted and the successful receipt is retained beside the file. The already delivered `Places-0.13.0-GPU-and-loading-before-after.mp4` remains intact at `local/loading-memory-0.13.0/montage/` (Library `libfile_259ae60e0de881919dd167a21a53422d`, file `file_0000000046f08230bd461aec64ccc002`). This new artifact adds verified evidence rather than replacing the previous delivery.

### Final gates, cleanup and handoff

| Acceptance item | Final result |
| --- | --- |
| Shared-engine pass 1 | Scoped CPU operation/parity evidence reused; four matched native scenes and focused lifecycle checks pass. Presented FPS gain inconclusive. |
| Render-preparation pass 2 | Ordered GPU-vertex/CPU preparation evidence reused; native visual parity and graphics/reset lifecycle pass. Presented FPS gain inconclusive. |
| Editor pass 3 | Native cold/warm picker, theme/search/no-match, selection/history, project open/reopen, level switching, preview and bake-state checks pass. Native grid pixels match exactly; CPU probe savings remain distinct from native latency/FPS. |
| Eight quests and hub reward | Ordinary native completions, held-item/contact evidence, later-session persistence, early locked door, idempotent NPC callbacks, Jun finding/walking/seating and final furnished-room return pass. |
| Menu left edge | Five native resize drawables plus fullscreen pass; title/navigation masks are unchanged. |
| Packages and editable custody | Final showcase all nine members current/integrity-valid with Full lighting; 20 recovered files byte-exact. Polished furniture current; both furniture packages and original captures retained. |
| Windows/Linux | Not run; unverified. |

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, and `cargo build --locked --release --workspace --all-features` all exit **0**. The test run passes **271 tests, zero ignored**, and tests pass again after the final Jun source correction. No tests are added or expanded. The player/editor/compiler hashes are unchanged by the final gates, retaining the actual binaries used for the accepted native runs.

All validation and owned native/build processes finished before cleanup. Eight release executables were copied and SHA-256 checked outside `target` in `local/final-verification-0.13.0/preserved-executables/`; target contained no unique map packages, captures or other recorded media. `cargo clean` exits **0**, reporting **25,301 files / 27.3 GiB removed**. Allocated target size changes from **20,403,228 KiB** before clean to **58,436 KiB** after restoring only the eight byte-exact executables (**59,822,528 logical bytes**) to their original `target/release/` paths. No build caches are restored. The original native-editor launcher remains unchanged and its compiler/game paths remain usable. Launches retain the host's existing SDL3 library dependency; portable packaging was not validated.

`post-clean-native-smoke/` successfully launches the restored player against the final showcase hash, reaches the actual Hub renderer, preserves the real eight-entry save, reports no script diagnostics, and shuts down normally. `cleanup.json` records preservation, idle-process checks, clean output, restored hashes and this launch. `Play-showcase-fresh.command` and `Play-showcase-completed.command` in the evidence directory provide isolated, VSync-off launches at the original executable path. The completed helper's save is a byte-exact copy of the real accepted Arcade completion save; no completion flags are manufactured. Both original/polished furniture packages, recovered editable projects, prior baseline/candidate executables, unique captures and both delivered montages remain outside `target`. The unrelated Python audit drafts remain untouched.

Exact source/report commits, remote confirmation and released repository/target ownership are recorded in `local/final-verification-0.13.0/FINAL-HANDOFF.json`. No version bump, release, main merge or force push is performed. GitHub CI is neither queried nor awaited; its separately authorized final-version/pre-bump gate remains deferred. No access action is required for the verified native workflows. The historical unused Validation.app approval remains unknown/untouched. Windows/Linux, uncapped presented throughput and the historical **+312 MiB total-residency observation** remain open limitations; the scoped owner-unload checks do not establish a return to baseline process/GPU residency.


## Portal 1-style keyboard and mouse movement

Implemented independently on Windows from the frozen staging baseline
31cb5e5415f7f054a631bea6c98305d382f0ff8e. The reference is Portal 1, replacing
the earlier Half-Life 2 wording. Keyboard and mouse controls remain the only
input scope. No portal gun/mechanics or gamepad support is introduced.

### Reference provenance and parity limits

The bounded read-only check found one registered Steam library. Its Portal
directory contains remnants but no executable; app 400 has no installed
manifest. No game was bought, downloaded or installed, and Steam/account
settings were not changed. Consequently there is no current-retail build,
platform/tick/cvar/plugin measurement to claim.

The pinned [SPT predictor declarations](https://github.com/OutOfBoundsOffice/SourcePauseTool/blob/d25c0edede1ad8db938874877180282350c169bc/spt/features/tas.cpp#L88-L103)
manually select air acceleration 15, a directional wish cap of 60 and retained
surface friction for Portal. These are predictor overrides, not verified retail
defaults. The [archived build-5135 fixture](https://github.com/OutOfBoundsOffice/SourcePauseTool/blob/d25c0edede1ad8db938874877180282350c169bc/Tests/portal%20-%205135/test/04_test.td)
and [input script](https://github.com/OutOfBoundsOffice/SourcePauseTool/blob/d25c0edede1ad8db938874877180282350c169bc/Tests/portal%20-%205135/test/04_test.srctas)
supply the 22.5-unit initial ground increment, seven 15 ms vertical velocity
landmarks (151 through 97, decreasing by nine), and an observed 75-unit forward
jump increment. They are archived harness evidence, not a clean modern retail
capture. The focused tests compare these velocity landmarks; no unavailable
retail position trajectory is invented.

No Valve SDK movement implementation or Valve-attributed SPT jumpboost hook is
incorporated. Portal-specific high-speed AirMove restrictions remain unresolved.
The isolated 15/60 projection model is provisional outside the documented
landmarks; it is not presented as complete Portal AirMove, unrestricted HL2
parity, a verified Portal 2 cutoff, or exact 1:1 retail movement. Contact ties,
stairs/view smoothing and detailed retail duck timing still need measured
reference comparison.

### Controller and authoring contract

One uniform scale is 0.025 metres per Source unit. The selected profile uses:

| Quantity | Source units/profile | Places |
| --- | --- | --- |
| Fixed command | 15 ms | 66.667 commands/s |
| Default land / crouched wish speed | 150 / 50 | 3.75 / 1.25 m/s |
| Ground acceleration / friction / stop speed | 10 / 4 / 100 | same rates, stop 2.5 m/s |
| Air acceleration / directional wish cap | 15 / 60 | same rate, cap 1.5 m/s |
| Player gravity / jump impulse | 600 / 160 | 15 m/s² / 4 m/s |
| Step | 18 | 0.45 m |
| Fixed square hull | 32×32×72 / 32×32×36 | 0.8×0.8×1.8 / 0.8×0.8×0.9 m |
| Standing / duck eye | 64 / 28 | 1.6 / 0.7 m |
| Ground duck / unduck eye transition | 0.4 / 0.2 s | same |

The existing adjustable walking preference remains available; comparisons here
request 3.75 m/s in both players to isolate controller behavior. Acceleration
limits velocity projected onto wish direction, retaining perpendicular and
overspeed momentum. A fresh ground-command jump precedes friction. Airborne
presses are consumed and held Jump does not auto-hop. The facing-relative
jump budget preserves the archived forward increment and backwards overspeed
behavior without a universal walking-speed clamp.

Crouch is now held. Ground transitions anchor feet; air duck raises feet by
0.9 m while retaining hull top and eye. Expansion checks the full static and
live-body hull and retries when clear. Collision and stance checks run on
fixed commands, including rigid/frozen bodies; blocked momentum is spent rather
than retained until a render frame or obstacle release. World boxes, oriented
doors, overheads and gabled ceilings use the square footprint. Slide and raised
step routes compare actual planar progress. Walking uses real treads; terrain
contact cannot select an unrelated lower floor as permission to enter a ramp.
A bounded downward probe retains contact without upward teleportation.
Steep slopes with upward normal below 0.7 clip/slip rather than becoming
grounded jump surfaces.

Standing feet reach approximately 0.533 m; air crouch extends feet reach to
approximately 1.433 m. Editor boundary diagnostics and the map authoring guide
use that reach. Existing authored Relay and Home geometry is unchanged.
The new opt-in editable projects/movement-gym/ uses ordinary editor geometry
and existing PNG-backed materials. The explicit places-movement-check tool
writes 17 command trajectories distinguishing feet, eye, hull top and velocity.
Neither map baking nor the developer replay is added to ordinary tests/CI.

Water retains its compatible controller and 9.8 m/s² checked-exit gravity,
independent of land gravity. Its default standing swim cap remains 2.4 m/s.
This is water compatibility, not Portal swimming parity.

### Original reproduction, regression value and native acceptance

Before controller edits, the frozen executable reproduced the Relay stair
jump falling below the flight and recovering to the start (one recorded reset).
The matched 960x540 comparison recording later completed that old route with
zero resets; the original failure is cadence-dependent and is preserved as a
separate 1280x720 reproduction. Original Home evidence confirms switch targeting, folding-door progress 1.0,
mug pickup, carry and set-down. All frozen manifests, packages, binaries and
captures remain intact. Local diagnostic project copies remove only outbound
transitions/change-level bindings; geometry, relevant props and interactions
remain the authored originals. Failed setup attempts are retained and excluded.

Five focused CPU regressions add concrete coverage beyond the prior 272 tests:
fixed-command scheduling and timed keyboard/mouse replay; archived jump/air
momentum and debounce; hull/eye duck ownership and short presses; fast/corner/
overhead and live-constraint collision; and the actual Relay jump/contact/
recovery path. Tiny metadata fixtures require no assets, renderer or map bake.
Existing water fixtures account for the new hull clearance and command interval,
and stop at their real bank acceptance point before the faster land speed
leaves the small fixture. Existing physical contact assertions derive their
bounds from hull width. No regression is ignored or hidden behind a feature.

The complete controller passes strict workspace Clippy, all 277 workspace tests
(zero ignored), and the locked all-feature release build on Rust/Cargo 1.99.1.
The release executables used for native evidence are preserved outside target.
A final developer-only replay spawn correction starts outside the low slab,
instead of recovering onto it; it changes no controller behavior. The final
tree passed the same gates again (clippy-probe-final.log, tests-probe-final.log,
release-probe-final.log). Command replays from that tree are preserved separately
from the earlier traces. The new gym's diagnostic floor alone extends west to
X=-8 so both mirrored paths stay within the fixture; the earlier left path
ran off its boundary. Published traces match mirrored endpoints and the
0.44/0.46 m step threshold. Prior gym packages used for comparison video are
retained with their own identities. No existing authored level is changed.
Native acceptance is detailed in native-final-audit.json: Relay stairs/ramp jump and land on authored supports without reset; Home switch, complete folding-door travel, mug carry/set-down and untouched dormant props pass; void recovery resets once; noclip exit falls and lands; both Pool exits reach dry ground; low slab blocks unduck after release; editor opens the gym. The unchanged After Hours 1.15 m passage is traversable crouched, refuses expansion inside and stands after exit with zero resets.
The exact final rebuilt player also repeats Relay stairs/ramp and Home smoke
checks successfully; its SHA-256 and receipts are in
published-build-comparison.json and native-final-audit.json. The final gym
compiles and opens in the published native editor.
No accepted eight-quest/bowling/sun-moon implementation campaign is repeated.

### Comparison video and timing scope

The requested comparison is saved as
local/windows-validation-0.13.0/portal1-movement/Places-Portal1-style-movement-comparison.mp4
and Library libfile_77c7435eee4c8191a1d6f38283da705a. It is H.264/YUV420P, 1280×528 at
30 FPS playback, 1,461,753 bytes, SHA-256 dc4f301f0c235be29340abaf6ac845140e9f83f3ab7a2265aa213bc6551cdf85.
Five matched input schedules show acceleration/stopping/reversal, air strafes
and hop/held-jump timing, air crouch reach, low clearance, and unchanged Relay
stairs. It labels old Places versus Portal 1-style Places; no actual Portal
footage is represented. Native frame sources, identities, timestamps and encoder
receipts are retained. Playback samples follow captured real elapsed time;
there is no time acceleration. Recording FPS is separate from measured game
FPS, and every recording run is excluded from throughput measurements.

Capture-free matched timing uses VSync Off, no application limiter, verified
applied presentation mode and physical drawable size; matched-timing.json and
per-run stdout/frames.csv retain actual values. All four tiny-gym runs applied
Immediate mode at physical/logical 1280x720 on NVIDIA RTX 3070 Laptop / DX12.
They ran above display refresh, with no pacing plateau. After 120 warmup rows,
360 rows per run gave mean complete-loop times of 1.234 / 1.318 ms for the old
player and 1.162 / 1.269 ms for the new player, in old/new/new/old order. Mean
update times were 0.0208 / 0.0179 ms old and 0.0168 / 0.0181 ms new. These are
short controls on a tiny scene; this work makes no overall FPS gain claim.
Refresh-capped earlier presentation evidence is not reused as throughput proof.
The earlier transient executor disconnect is recorded separately from
build state; resumed validation checked existing processes before continuing.
Two owned failed-load capture processes were closed using their saved pending
receipts, preserving all evidence.

All evidence for this section is beneath
local/windows-validation-0.13.0/portal1-movement/. No executable bundle is
uploaded to Library, no target clean occurs, and there is no version bump,
main merge, release or force push. Keep-awake ownership remains with the
approved queue. Exact commit/remote identity and handoff are recorded locally
after publication.

Library confirmed the MP4 save and version 0. Its returned metadata is retained
in comparison-library-identity.json; applying local extended attributes failed
because Windows Python has no os.setxattr. This does not undo the confirmed
Library save and no duplicate upload was made.

### Approved appearance follow-up transport blocker

The approved five-skin ZIP is Library libfile_db44efe245d8819180bdb9b202869d42,
version 0, 3,621,259 bytes, expected SHA-256
48d3998ab303e3a4e3c55c1b7b6af15c2c267f4aba8ba738c7af00d47c87ad8a.
Library preparation succeeded. The supported current transfer helper then
downloaded into its temporary staging flow but failed at os.setxattr, which is
unavailable on Windows Python, and left no materialized ZIP. This is a helper
compatibility failure, not an access denial. The initial sandbox network failure
and subsequent authorized-network metadata error are retained in task-2's
skin-materialize logs. No unsupported transport, invented cloud filesystem path,
replacement imagery or partial catalog entries were introduced. Existing skins,
shared GLB, rig and clips remain intact. Integration and its required native
five-appearance persistence/animation/ragdoll validation await a supported local
materialization path; no appearance integration commit is claimed.


## Approved GIMP mannequin appearances

The five approved GIMP sheets are integrated through the existing shared R2
appearance path. Man, Woman and Zombie retain their original catalog IDs;
`core:mannequin_skin_robot` and `core:mannequin_skin_scientist` add the Robot
and lab-coat Scientist choices. Original and Black tint remain available.
Both placement and spawn-template choosers expose all seven options, and
`character_texture` persists through save/reopen and project export/import.
The catalog additions retain the existing entity / texture / file record shape.

### Approved transfer and exact asset ownership

The earlier Windows Library helper and unauthenticated browser failures remain
preserved as historical attempts. Explicit user approval enabled an asset-only
GitHub transfer. Commit a41f6d3cb22b81ce66d5bb8177547a098e75e166 is a direct
child of movement commit 1a32eb48023f80d19ae291f2fa10886d583508e0 and changes
exactly five PNG paths. It was fetched and fast-forwarded only after local
cleanliness, exact parent and path-set checks; cloud writer ownership was
released before native integration resumed. No concurrent repository writers,
Library-original changes, metadata removal or helper bypass were used.

The parent verified GitHub reads against original approved GIMP bytes. Native
validation independently checked each local Git blob against that exact asset
commit, its PNG IHDR, dimensions, mode and fully opaque alpha. All five remain
1024x1024 RGBA8 with the original fitted UV layout and orientation:

| Sheet | PNG bytes | SHA-256 |
| --- | ---: | --- |
| Man | 733028 | 2b21bdb35cb8a9dad7d608f235846990f866dae769dcd4ac84e03e7b375b2ab0 |
| Woman | 747274 | b9f194d29919e9bf95bf511d9e74abb946dc9ed1cb6e2c04666e9ebf2137502d |
| Zombie | 716037 | 65d185d2db72b5666d14f5d756fcc7761a05686f730249ca0dbdb10e7f2b89ad |
| Robot | 709979 | a2f9ed3f93a626ce7002ea6bff8cddd8be948fffffeb93360f4291698b9a7151 |
| Scientist | 720779 | 2f4c57c7fd90ee80ace5e58a337c74180b23c731cbe687156d2171a902696a14 |

The unchanged shared GLB SHA-256 is 6627807d4cddf7331cc7f96b03adb43f3686af71366bc8dc257f881b6ddf1156.
No model, geometry, UV, rig, joint material or animation file is duplicated or
re-authored. The accepted 37-joint / 19-clip R2 remains the owner of animation;
the first skin submesh receives the selected albedo and recessed joints keep
their existing material. Layered GIMP sources remain in Library
libfile_d00b41d9bb3c81918519fcc8f476d1e8; no XCF enters the runtime package.

### Focused Windows acceptance

The opt-in `projects/mannequin-appearances` review now contains all five
approved skins with independent walk/run playback, plus Original and Black.
Two additional actors and reset targets extend the existing review fixture;
it remains outside ordinary Rust tests and CI map baking.

Native editor input cycles every appearance for both a placement and a spawn
template. Checkpoints verify exact IDs, removal of the optional texture for
Original/Black, and the unchanged Black preset model. The saved document
matches a fresh editor reopen. Export/import source files are byte-identical,
and the recovered project opens in the native editor. Native player captures
show each of the five skins animated, then independently entering a 15-body /
14-joint ragdoll; concurrent counts progress 1 through 5 and reset to zero.
Original and Black receive separate native views. A Scientist spawn template
creates an additional actor whose animation continues independently. The review
template explicitly starts its walk loop with the supported `spawn` binding
and `play_animation` action, rather than assuming animation component metadata
automatically starts renderer playback. The initial standing-spawn capture is
preserved separately; no engine behavior is changed to set up this fixture.
No script error or player recovery reset is observed in the acceptance runs.

The final tree passes strict all-target/all-feature workspace Clippy, all 277
workspace tests with zero ignored, the locked all-feature release build,
format/diff checks, and the 629-entry asset validator with zero warnings.
No redundant Rust test is added for two new catalog/match choices; the focused
native checks directly cover the UI, serialization and actual renderer paths.
VSync is off with no application limiter during native runs; renderer/window
receipts retain the applied presentation mode and drawable size. These visual
captures are not used to claim a throughput or FPS improvement.

Evidence and scripts are retained outside target beneath
`local/windows-validation-0.13.0/skin-integration/`: approved-asset-provenance.json,
native-final-audit.json, editor/native captures, export/import artifacts and
build logs. The registration/report commit follows the approved PNG-only
commit separately. Exact publication identity and writer release are recorded
in ownership-handoff.json after push. Target and the queue-owned SYSTEM-only
keep-awake helper remain available; no next game mode starts in this task.

## One Guest Too Many (#26)

The approved single-player birthday-party mode is complete in the editable
`projects/one-guest-too-many` project. The 14 x 16 m room contains a birthday
table, six chairs, gifts, balloons/streamers, a furnished arcade return route,
entrance hall and lockable waiting room. All geometry, 46 existing Arcade/Home
props, six lights, door, spawn markers and two trigger volumes were created
through the real native editor controls (80 scene objects). Supported source
components, bindings and the Luau module supply behavior. No new texture,
model, rig or renderer-specific gameplay path was introduced.

The first party entry and each completed arcade return admit one mannequin,
capped at six. The third arrival is the same hidden follower on every replay;
all use Original appearance, so movement and doorway following provide the
clue. Five guests remain harmless. Collision-based sight is sampled at 10 Hz
within 12 m, with 360-degree awareness and no hearing. Losing sight searches
the last seen position for four seconds, then stops; a new sighting resumes
navigation. Existing navigation and swept collision carry the follower around
furniture and through actual doorways. A visible 2.5-second room-survey pause
gives the player time to leave and operate the external latch.

Containment requires the actual settled, unobstructed, locked leaf, the player
safely outside and a conservative full body envelope inside the waiting room.
The 0.4 m radius / 1.9 m navigation body receives an additional 0.4 m horizontal
containment margin for the shared stand/walk poses. The cake checks this proof
again when used after all six arrivals. An obstructed leaf, empty room or
harmless guest cannot win. The outside latch cancels/reopens a pending or
locked door; an inside player can release the handle. Premature cake use has
only an in-world light response. There are no tutorial/hint panels or quips.

Restart, Enter replay, menu reentry and application reload restore arrivals,
identity, AI memory, doors, locks and progression. The hall switch resets a run
at any time. This mode writes neither campaign completion nor an in-progress
save. Its README documents the deliberate deterministic identity and volatile
save contract. No random appearance or unexplained identity change affects a
run. Existing eight-favor content, menu events, sun/moon, skins, movement,
physics and water source/assets remain intact.

### Shared capabilities and native acceptance

Luau gains read-only `door_status` (actual phase, lock, obstruction and angle),
navigation-body dimensions on `actor`, and a typed, budgeted `prompt` action.
Scripted packages declare `world-observations-v1`. These use existing world
state and deferred atomic actions. The only renderer change adds current posed
vertex bounds to explicit native trace diagnostics for containment auditing.
No ordinary renderer path changes. The essential metadata-only regression
`observations_report_live_door_state_and_replay_atomically` distinguishes a
queued lock from settled state, checks immutable observations/atomic refusal
and restores authored state on reset. It needs no GPU, assets or map bake.

| Native case | Verified result |
| --- | --- |
| Final Full ordinary-input run | Authored spawn, six real return crossings, pursuit, lure, real external lock, cake victory; no player/actor pose, navigation-goal or direct-activation diagnostics. |
| Repeated visits and reset | Six extra return loops stay at six AI / nine total render actors; reset leaves zero AI / three baseline render actors; three fresh arrivals recreate `guest_3#3`. |
| Lose/reacquire | Occluded player behind waiting-room walls is not tracked. Follower stops at last observed location, then resumes after a visible reappearance. |
| Wrong guest / reopen | Harmless guest physically navigates into the room in a labeled edge setup; locking does not win, cake refuses, reopening releases the guest. |
| Door obstruction / player escape | Real follower at the threshold stalls closure at -20.204 degrees, obstructed and unlocked. Cancel/reopen recovers. Locked leaf stops the player sweep; inside lock request refuses, inside handle and hall reset remain usable. |
| Whole body / final proof | Final posed bounds X 2.859..3.291, Y -0.00014..1.723, Z 13.323..13.805 lie inside the room tolerance; leaf is closed at 0 degrees, locked, unobstructed; player Z 9.791 is safely outside. |
| Replay / same-profile application reload | Enter replay and reload after victory recreate zero spawned AI, three baseline characters, closed unlocked door and no outcome. |
| Preserved carry workload (#28) | Existing Home fixture uses real pickup/animated carry sockets and visible item geometry. Combined body/prop bounds remain visible until the entire assembly is hidden; retirement then removes the prop. Existing exposed-limb/prop regression remains passing. |

Labeled edge setups use diagnostics only to reach unusual wrong-guest and
obstruction conditions; they are separate from the ordinary-input victory.
Accepted cases contain no disabled Luau VM or unrequested player recovery.
Spawn count is bounded at six, there is one mode VM, callbacks are fixed and
the highest observed VM allocation in accepted snapshots is 682,968 bytes.
Independent actor/carry animation and whole-body bounds are retained in traces.
These are observed/resource and configured bounds, not a system-RSS claim.

Native editor Save+Build, actual Build+Play child, export, editable import and
reopen all pass. Recovered manifest, level and Luau bytes exactly match source.
Static/off and Full variants are present. Geometry validation reports zero
errors and warnings. A first verification wait expired during a normal Full
bake; staged progress waits completed the same small map without changing
security/permission controls. The large showcase was not rebaked.

### Matched Windows workload counters

Final executable/package, NVIDIA RTX 3070 Laptop on DX12, Ryzen 9 5900HX / 16 GB,
physical drawable 1280 x 720, high texture/filter settings, Full lighting and
reflections off. VSync is off, no frame limiter or fixed delta is active, and
every sampled resource receipt reports actual `Immediate` presentation. Samples
are above the 165 Hz display rate. Builds, baking and capture/recording are
stopped during timing. Each row uses the last 1,200 frames after settling;
ABBA ordering compares zero versus six admitted guests from the same foyer
camera. Only a subset is visible. This measures added workload, not a claimed
FPS optimization or an all-guests-visible worst case.

| ABBA run | FPS | Mean loop ms | p95 ms | Update ms | Render submission ms | GPU timestamp ms | Draws / vertices / binds |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Empty A | 408.03 | 2.451 | 4.588 | 0.103 | 0.548 | 1.442 | 43 / 32136 / 34 |
| Six A | 335.13 | 2.984 | 5.376 | 0.189 | 0.601 | 2.447 | 51 / 49496 / 42 |
| Six B | 323.45 | 3.092 | 5.605 | 0.189 | 0.613 | 2.594 | 51 / 49496 / 42 |
| Empty B | 367.96 | 2.718 | 5.097 | 0.113 | 0.565 | 2.096 | 43 / 32136 / 34 |

Both empty runs retain 211 HAL buffers / 52,494,336 buffer bytes, 74 textures /
174,915,584 texture bytes and 488 bind groups. Both six-guest runs retain 227
buffers / 57,278,464 bytes, 75 textures / 180,551,680 bytes and 509 bind groups.
HAL totals exclude driver overhead; Windows RSS sampling is unavailable.
Run-to-run timing variation is retained. No cross-platform or integrated
showcase performance claim follows from this focused Windows workload.

### Gates, artifacts and handoff

The final code passes `cargo clippy --workspace --all-targets --all-features --
-D warnings`, `cargo build --locked --release --workspace --all-features`, and
all 278 workspace tests with zero failed/ignored. Format/diff checks pass; the
629-entry asset validator has zero warnings, and package dependencies validate.
No broad CI/full-map test dependency or additional report file is introduced.

Raw evidence remains outside target under
`local/windows-validation-0.13.0/one-guest/`: native-acceptance-audit.json,
matched-performance.json, final-evidence.json, native/editor scripts, snapshots,
traces, logs, recovered editable project and final executable hashes.
`OneGuestTooMany.placesmap` is the 15,498,636-byte normal editor export;
SHA-256 `2ef957413cdf73b804db1351a1520194c618b5554be9bfddb4a661f46b27116a`.
`OneGuestTooMany-native-gameplay.mp4` is a short 1280 x 720 native pursuit/lock
clip, H.264, silent, 15 fps, 388,075 bytes; SHA-256
`d9101a983ee47dd81f7de63a492a8700b756bf571f49f53d62107a39eed82651`.
The two distinct snippets preserve actual capture elapsed time; diagnostic
setup is omitted between them. The ordinary full victory is separately traced.
Video timing is excluded from performance samples. Initial unsuccessful timing
and setup attempts remain preserved and are not counted as acceptance passes.

No functional blocker remains in this Windows scope. Other platforms are not
rerun. Exact publication identity, clean local/remote state and writer release
are recorded in ownership-handoff.json after push. Build artifacts and the
temporary SYSTEM-only keep-awake helper are retained for the queue. Dynamic
blood is next; Shape of an Exit remains second-to-last, before final verification.
No next task, issue closure, main merge, release or version bump occurs here.


## Dynamic blood effects: engine, Luau and native acceptance

October 23, 2026. Approved queue item after One Guest Too Many, starting from
remote-verified `0.13.0-staging` commit
`5b7ed98e9198ca522b137452e21b54e8a1eea5ff`. This adds a reusable presentation
capability. No shipped map script, map gameplay, weapon/health system, wave
shooter, version, or release is changed.

### Implemented contract

`api.blood(x, y, z, dx, dy, dz, droplets?)` traces a finite impact segment;
the direction is an offset in metres, rather than a normalized direction.
Origins must lie within +/-100000 m; segment length is 0.001..12 m.
`droplets` is an actual numeric integer in 0..12, default 8. Strings,
non-finite values, invalid rays and fractional/out-of-range counts are refused.
The existing scoped Luau call/action budgets and transactional handler rollback
apply. The corresponding generic authored action is
`{"action":"blood","emission":{"origin":[0,1,0],"direction":[0,-2,0],"droplets":8}}`.
Emission is opt-in; it does not assign damage rules to existing maps.

The nearest struck displayed triangle receives a small asymmetric faceted
mark. Clipping stores barycentric coordinates on that triangle, so its corners
follow the same posed vertices as the character upload during animation and
ragdoll ownership. Stable birth identities prevent a reused script id inheriting
another actor's marks; compatible graphics rebuilds retain the identity.
Rigid props retain their live object id and local triangle indices. Ordinary
depth testing supplies body/world occlusion; the geometric normal offset is
0.8 mm. No camera-facing surface decal can hover across a silhouette or joint.

Ballistic droplets sweep their displacement against the current static and
posed/rigid geometry. Static-quality packages receive a lazy immutable surface
BVH from retained geometry, independently of lighting transport availability.
The index excludes claimed character bind poses and uses the existing static
alpha-aware ray machinery. It builds once on demand, with no lights, probes,
receiver targets or solve. Each impact/mark update leaves lighting revisions,
lightmaps, shadow casters and the prepared world untouched.

Hard caps are 8 queued requests, 96 live droplets and 256 live marks, with
oldest-first eviction. In active simulation time, droplets live at most 2 seconds;
marks live 30 seconds and fade over their last 5 seconds. Pause freezes that clock.
Long suspended frames discard droplets
instead of tracing a large catch-up jump. The existing effect pipeline and
committed white PNG draw geometry-only dark-red pigment: no raster artwork is
generated and no asset/UV contract changes. A lazy fixed 8192-vertex buffer is
294912 bytes; at most one additional draw submits the effect geometry.
Empty buffers may remain cached after fade; disable/reset/unload release GPU
and surface-index ownership. Scene queues and logical effects clear as well.

Graphics / **Blood Effects** is a saved On/Off preference, default On for old
settings. Off immediately clears existing effects, releases their GPU/index
resources and suppresses later emissions. Reset, same-map reload and menu unload
clear pending emissions and all live effects. A texture-quality rebuild keeps
valid attached marks and never needs a lightmap bake for them.

### Native validation and gates

Final binary SHA-256: `1587d482f5399c157ba31400ea3e3037d9e2c885cb514c6f4c290f6a4622ad28`.
All 22 acceptance checks in
`local/windows-validation-0.13.0/blood/native-acceptance-audit-ship.json` pass.
Timestamped native framebuffer PNGs and live-triangle/identity observations
verify walking attachment, physical ragdoll attachment, floor/wall impacts,
secondary droplets landing, translating/rotating table attachment, Low quality
rebuild preservation, fade, reset, reload, unload with active marks, live Off and
persisted disabled launch. The stress run reaches exactly 256 marks and 96
droplets, then expires to zero. Real opaque-wall comparison retains live marks
while the wall hides them, then reveals them again. Camera/control diagnostic
setup is disclosed in each actions.json; this is not a shipped gameplay mode.

The initial `native-smoke` exposed a Static-quality world-index omission. It is
preserved, fixed, and covered by a three-vertex no-lighting-field regression.
An early outside-room occlusion view fell too low; the explicit final wall
comparison replaces that view as evidence. Earlier candidate binaries and
receipts remain separate from the final `*-ship` runs.

Strict `cargo clippy --workspace --all-targets --all-features -- -D warnings`
passes. `cargo test --workspace` passes all **282** tests, zero failed/ignored;
execution is 1.02 s. Four focused blood regressions protect clipping/swept hits,
bounded lifecycle, Static-quality collision, and the real transactional Luau
boundary including strict numeric count input. Fixtures contain only tiny
geometry/metadata and introduce no automatic map-baking requirement.
`cargo build --locked --release --workspace --all-features` passes (6m 35s).
Format and diff checks pass; the 629-entry asset audit has zero warnings.
The isolated existing-asset fixture compiles in Static only and validates with
intact dependencies. No expensive lightmap bake or full-map campaign ran.

### Matched uncapped performance

Windows, NVIDIA GeForce RTX 3070 Laptop GPU, DX12, physical drawable 1280x720,
165 Hz desktop. Every timing run records actual **Immediate** presentation and
swap interval 0, VSync Off, no limiter. Build/compiler/encoder/recording processes
were stopped before timing. Each matched control uses the same binary, package,
camera, actions and settings. The first four runs compare Blood Off with a settled 256-mark ring, ABBA order.
Their 7..13 second samples follow setup/emission and particle settling. The last
pair samples the same window during continuous capped emission (eight requests
of twelve droplets every 200 ms). Throughput exceeds display refresh;
this is not a refresh-rate plateau.

| Run | FPS from mean loop time | Mean loop ms | p95 loop ms |
| --- | ---: | ---: | ---: |
| perf-off-a | 665.38 | 1.503 | 1.682 |
| perf-on-a | 646.59 | 1.547 | 1.794 |
| perf-on-b | 626.76 | 1.596 | 1.871 |
| perf-off-b | 627.59 | 1.593 | 1.846 |
| perf-stream-off | 664.45 | 1.505 | 1.667 |
| perf-stream-on | 643.05 | 1.555 | 1.939 |

For settled marks, Off mean loop time is 1.548 ms; On is
1.571 ms, difference +0.023 ms
(+1.48%). These are measured workload costs with run noise,
not an optimization-gain claim; between-run variation is larger than that small
aggregate difference. The continuous-emission pair measures
1.505 ms Off versus
1.555 ms On,
+0.050 ms
(+3.33%). It is one matched pair.
Raw CSVs and actual presentation/resource receipts are retained in
`matched-performance.json` and the six perf folders.
HAL-reported bytes exclude driver overhead; no Windows RSS or whole-game memory
claim is made. This small fixture does not establish performance for every map.

### Media and exact limits

`Places-dynamic-blood-native.mp4` is 838233 bytes, 1280x720, silent H.264,
8 fps and about 11.88 seconds. It uses timestamped native framebuffer
samples at their actual elapsed times, with disclosed diagnostic camera cuts;
it is not a 60 fps recording. Decode verification passes. SHA-256:
`a3f6fc1744c48bbfbf0458937bb0ff73aab5333ff554abfbe6d37aa0be338675`. The local recording, source frames,
video receipt, native/audit tools, isolated source project, packages and final
executables are retained outside target under the blood evidence directory.
Video capture and encoding are excluded from FPS measurements. No executable
bundle is uploaded. The video is saved to Library; its authenticated reference
is retained in the handoff and library-transfer.json. Windows cannot persist the
helper's local extended attribute, so that receipt preserves the identity mapping.

Marks stay on one struck triangle: they clip at mesh seams/corners instead of
spreading across connected topology. They are flat pigment in the effect pass,
without lightmap/BRDF relighting, reflections, fluid pooling or flow. Opaque
geometry supplies depth occlusion; blood is not sorted or attenuated through
blended glazing. Moving model receivers support opaque primitives; transparent/cutout moving primitives
are excluded. Static rays sample existing alpha at the hit, while the polygon
is clipped to geometry, not a per-pixel alpha stencil. Emissive fixture faces
and transmissive water are not blood receivers. Particle sweeps use the current
posed scene, not continuous moving-mesh collision between poses; fast objects
crossing a droplet between frames can be missed. The immutable surface index
uses the existing transport triangle budget and has a first-emission setup cost.
Native execution is **Windows/DX12 only**; macOS, Linux and browser behavior is
not claimed. Remaining queue-wide performance/platform work stays deferred.

Exact commit, remote verification, clean state, keep-awake custody and writer
release are recorded after push in `blood/ownership-handoff.json`. Build
artifacts and SYSTEM-only helper PID 18136 are retained. No CI wait, main merge,
release, version bump, security change or next queue task occurs here.

## GPU deformation and exact pose reuse — Windows acceptance, 2026-10-23

This phase moves eligible skeletal vertex deformation to a compute pass while
retaining CPU animation, sockets, blood/geometry queries and collision. It folds
the rendering residuals of [#27](https://github.com/csd113/Places/issues/27) and
[#31](https://github.com/csd113/Places/issues/31) into the approved queue.
The preserved baseline is staging commit
`4790ecf6ec14f994f0cb3bdab541eaaefac6451b`, player SHA-256
`1587d482f5399c157ba31400ea3e3037d9e2c885cb514c6f4c290f6a4622ad28`.
Accepted player SHA-256: `568f0de8c4aefbc9a0eaaf46c89351255733fb336756ffb4b8bfb5645f458f29`.
The final commit and clean remote verification are in the ownership handoff.
All evidence below is under `local/windows-validation-0.13.0/gpu-rendering/`.

### Implementation and bounded scope

- One immutable bind-data buffer per model and a retained per-actor joint palette
  feed compute deformation into the existing world vertex buffer. One result is
  consumed by scene, shadow and reflection passes, before any view draws it.
  The shader derives the actual 104-byte vertex stride from Rust and changes only
  position and inverse-transpose normal fields. Per-instance lighting, placement,
  opacity, appearance, indices and material bindings retain their ownership.
- Exact reuse requires the same model, full bit-identical matrix palette and
  albedo bytes. Revision counters alone never establish equality. Actors keep
  independent animation clocks and buffers; alias splits restore their own
  output before drawing. No pose quantization, cadence reduction or animation LOD.
- Existing conservative joint-influence bounds replace the per-vertex render
  bounds scan. Animation, sockets, held items, ragdolls and collision continue to
  use current CPU poses. Hidden actors catch up before returning onscreen;
  pending compute survives skipped presentation. Existing culling is retained.
- Small meshes (<1024 vertices), morph targets, translucent sorting, UV-reactive
  meshes, tiny weights and unsafe/extreme/non-affine/ill-conditioned transforms
  retain CPU deformation. Ordinary reflection and nonuniform scale are accepted.
  Buffer-binding/workgroup limits gate eligibility. Existing bounded CPU worker
  dispatch remains for fallback; no job-system rewrite.
- Mutable vertex-color revisions trigger a full refresh, then return to compute
  on the next changed pose. Full Arcade's balloon tint exposed this dependency
  in the new readback oracle. An earlier hardcoded 100-byte stride also failed
  visual review. Both failed candidates and evidence are retained; neither is
  included in final measurements. The existing tiny wire-field regression now
  also protects tint changes with an unchanged palette and offscreen restoration.
- Runtime character admissions reuse the deformation pipeline on the same device.
  No texture, map source, package format, compiler algorithm, quality or version
  changes. The final source includes only renderer/character ownership, two small
  numeric/equality regressions, the existing fixture extension and diagnostics.

For the twelve 4340-vertex mannequins, a changed frame's nominal position/normal
upload path falls from 5,416,320 full vertex bytes to 28,416 palette bytes
(12 × 37 × 64), before exact-output reuse. Shared immutable bind input is
277,760 bytes/model. These are logical payload calculations, not measured VRAM
or RSS savings: per-actor output buffers and CPU fallback data remain resident.

### Matched uncapped throughput

Native Windows/DX12, NVIDIA GeForce RTX 3070 Laptop GPU, driver 617.14,
physical drawable 1280×720, windowed, standard style, High textures/filtering,
bloom on, FOV 60, Blood on. Static and Full lighting are separate cases;
Static intentionally disables the engine's Full surface-response/reflection
features in both builds. Requested Full reflections are otherwise retained.
Rust/Cargo 1.99.1; `--locked --release --workspace --all-features`, opt-level 3,
LTO, one codegen unit, unwind. No debug-build comparison.

All 52 final runs use VSync Off, no limiter, real delta and isolated state.
Actual HAL frame receipts verify **Immediate** presentation and 1280×720 in
every run; benchmark receipts verify swap interval 0 and no missed ready
presentations. Fast cases run well above the 165 Hz desktop reference, excluding
a global refresh plateau. Any individual slow/near-refresh view is interpreted
with its frame-time spread and GPU cost, not assumed to prove a gain.
Optional WMI CPU/OS/display inventory returned Access denied; it was not retried
or bypassed. Exact CPU model/OS build inventory is unavailable. Native
action-result telemetry independently records a 2560x1440, 165 Hz display and
the 1280x720 drawable.

Each case uses ABBA order (baseline, candidate, candidate, baseline), identical
package hashes/settings/camera/actions. Ordinary runs last ten ready seconds;
the 4..9 second steady window excludes startup and warmup. CSV rows align
one-to-one with actual ready presentation trace timestamps, anchored to ready.
The initial provisional CSV-cadence accumulator could count initial loading
twice; its JSON/video is retained under provisional-clock and superseded here.
No runs were repeated for this analysis correction. Six-guest runs last
39.33 seconds and sample 20..35 seconds after actual admissions. FPS is
1000 divided by the mean of the two per-run mean loop times. Tails below average
the two run percentiles; raw per-run p95/p99, sample counts, GPU observations,
presentation receipts and settings are in `matched-performance-final.json`.
Builds, bakes, framebuffer capture, GPU oracle readback and encoding are stopped
before these measurements; `timing-preflight.json` records that boundary.
No loading improvement is claimed.

| Workload | FPS before / after | Mean loop ms before / after | p95 loop ms before / after | Frame-time reduction |
| --- | ---: | ---: | ---: | ---: |
| 12 mannequins + Luau, Static | 255.04 / 297.45 | 3.921 / 3.362 | 4.844 / 5.169 | +14.26% |
| 12 mannequins + Luau, Full | 234.32 / 266.68 | 4.268 / 3.750 | 5.241 / 6.031 | +12.13% |
| Home idle, Static | 458.41 / 456.52 | 2.181 / 2.190 | 4.182 / 4.205 | -0.41% |
| Pool closet, Full | 425.78 / 419.22 | 2.349 / 2.385 | 4.237 / 4.409 | -1.57% |
| Arcade bowling, Static | 256.99 / 291.86 | 3.891 / 3.426 | 7.304 / 5.092 | +11.95% |
| Arcade bowling, Full | 160.44 / 190.97 | 6.233 / 5.236 | 8.120 / 6.164 | +15.99% |
| Arcade mirror, Full | 195.47 / 197.51 | 5.116 / 5.063 | 7.107 / 6.078 | +1.03% |
| Overlapping fog fixture, Full | 531.44 / 527.64 | 1.882 / 1.895 | 3.814 / 3.833 | -0.72% |
| Non-overlap fog fixture, Full | 514.29 / 510.91 | 1.944 / 1.957 | 4.243 / 4.269 | -0.66% |
| Seeded menu NPC event | 245.13 / 243.33 | 4.079 / 4.110 | 7.851 / 7.986 | -0.74% |
| Forced menu ragdoll event | 226.14 / 245.88 | 4.422 / 4.067 | 5.801 / 7.521 | +8.03% |
| Animated/ragdoll blood fixture | 540.52 / 519.83 | 1.850 / 1.924 | 2.681 / 3.791 | -3.98% |
| One Guest Too Many, six guests | 309.92 / 313.64 | 3.227 / 3.188 | 6.390 / 5.736 | +1.19% |

CPU update includes simulation and render preparation; CPU render measures
submission. GPU values are deduplicated asynchronous hardware pass elapsed
coverage, including scheduling gaps. They are neither utilization nor additive
to the CPU columns; the candidate includes deformation in that coverage.

| Workload | CPU update ms before / after | CPU submit ms before / after | GPU mean ms before / after | GPU p95 ms before / after |
| --- | ---: | ---: | ---: | ---: |
| 12 mannequins + Luau, Static | 2.494 / 1.326 | 0.913 / 0.734 | 1.197 / 1.653 | 2.477 / 2.265 |
| 12 mannequins + Luau, Full | 2.839 / 1.833 | 0.870 / 0.844 | 1.445 / 1.958 | 3.127 / 3.056 |
| Home idle, Static | 0.134 / 0.134 | 0.578 / 0.577 | 1.224 / 1.204 | 2.745 / 2.748 |
| Pool closet, Full | 0.211 / 0.221 | 0.480 / 0.504 | 1.437 / 1.500 | 2.519 / 2.812 |
| Arcade bowling, Static | 1.482 / 0.985 | 1.049 / 1.130 | 1.778 / 2.227 | 3.891 / 3.738 |
| Arcade bowling, Full | 1.331 / 0.966 | 1.753 / 1.757 | 4.894 / 4.711 | 6.573 / 5.254 |
| Arcade mirror, Full | 1.311 / 0.963 | 1.291 / 1.410 | 3.722 / 4.754 | 5.258 / 5.719 |
| Overlapping fog fixture, Full | 0.029 / 0.026 | 0.501 / 0.472 | 1.107 / 1.140 | 1.784 / 1.720 |
| Non-overlap fog fixture, Full | 0.025 / 0.023 | 0.495 / 0.455 | 1.017 / 1.063 | 2.260 / 2.351 |
| Seeded menu NPC event | 0.844 / 0.700 | 0.788 / 0.865 | 2.768 / 3.193 | 4.556 / 4.790 |
| Forced menu ragdoll event | 2.807 / 2.103 | 0.992 / 0.930 | 1.409 / 2.095 | 3.090 / 4.640 |
| Animated/ragdoll blood fixture | 0.275 / 0.173 | 0.586 / 0.655 | 0.774 / 0.932 | 0.964 / 1.963 |
| One Guest Too Many, six guests | 0.213 / 0.221 | 0.645 / 0.637 | 2.652 / 2.648 | 3.809 / 3.224 |

The Static mannequin mean loop changes from 3.921
to 3.362 ms,
while its averaged p95 changes from 4.844
to 5.169 ms. The increased GPU work and
any worse tails are explicit tradeoffs of this CPU offload, retained for the
integrated performance/regression phase. This is not a blanket GPU-time reduction
or an all-scenes speedup claim.

The baseline/candidate blood comparison is retained as a regression, without
attributing its cause from two runs. A focused final-binary CPU/GPU ABBA repeat
(`blood-same-build-controls.json`, separate from capture/build/encoding) measures
1.652 ms / 1.646 ms,
605.31 / 607.58 FPS. That near-flat repeat
does not establish a consistent small-scene offload penalty or a universal gain;
campaign-to-campaign variation is material. No map-specific exception or
speculative scheduler rewrite is introduced to fit one result.

The accompanying raw runs expose repeat variation; small signed differences in
unchanged/single-actor cases are controls, not universal optimization claims.
CPU-stage trace profiles separately identify skin/upload preparation and animation
evaluation. They use diagnostic capture and are excluded from authoritative FPS.

### Visibility, interiors and fog findings

The earlier accepted conservative occlusion work (308 to 198 calls in its
documented matched case) already supplies #31's visibility mechanism. This phase
preserves it and checks live doors, offscreen return, carried geometry and Full
reflection views. A new room/portal architecture has no demonstrated incremental
benefit here and is not implemented. Draw/visible-vertex/reflection counts remain
available per final run, with conservative deformation bounds allowed to submit
extra safe work instead of risking missing geometry.

Full Arcade and its bathroom mirror, Full Pool water and the existing two-volume
fog fixture are profiled at matched quality. Arcade has no authored global or
regional fog; its historical slow views must not be attributed to overlap fog.
The current fog shader already bypasses empty fog and rejects rays outside each
region before four fixed quadrature samples, preserving strongest-volume overlap
and tie order. Fog sampling, lighting, shadows, mirrors and postprocessing are
unchanged. Distinct overlap/clear viewpoints characterize whole views; they are
not an isolated causal fog-on/off measurement and do not justify a fog-rewrite
or claimed fog-specific saving. No samples/effects are removed for throughput.

### Correctness, gates and limits

The final `color` binary cohort passes all 26 core native checks:
Luau visitor spawn/despawn and reload; twelve offscreen/onscreen animation clocks;
actual Home cushion pickup/carry/drop; Pool closet door opening and return;
Full water/flashlight; six One Guest Too Many admissions (nine total actors);
seeded and forced ragdoll/ghost menu events; animated/ragdoll blood triangles,
moving rigid-prop attachments, quality rebuild, fade/expiry/reset/reload/unload.
The separate Full Arcade/mirror/lab/fog captures pass the GPU oracle as well.
The oracle reads actual GPU output, including shared leaders, compares CPU
positions/normals, every invariant wire byte and conservative bounds. Core max
position error is 0.000000238; max normal error is
0.000000238; up to 4 shared outputs
were observed. Diagnostic readback deliberately waits and never runs in timings.
Existing tiny equality/numeric tests protect tail matrices, signed zero,
palette length and unsafe-transform fallback without GPU/assets/map bakes.

`cargo fmt --all --check`, strict workspace/all-target/all-feature Clippy,
`cargo test --workspace` (284 passed),
`cargo build --locked --release --workspace --all-features`, `git diff --check`
and the asset validator pass. Final gate logs are `clippy-color-fixed.log`,
`tests-color-fixed.log`, `workspace-color-fixed.log` and `assets-final.log`.
All build executables/DLLs are retained in `accepted-color-bin/` and target.
Earlier Windows replacement of the actively baking compiler was denied by its
file lock; no force/security bypass occurred. After the bake exited, the required
workspace build succeeded. There is no remaining permission/tool blocker.

Existing-asset Full Arcade/Lab and the tiny fog fixture were explicitly baked for
native acceptance, outside Rust tests/CI. A standalone Arcade member correctly
failed transition validation because it needs its complete project. The existing
lossless diagnostic composition workflow retained all complete-project members
and replaced only Arcade with byte-identical Full records; complete project,
Lab and fog packages then validate with intact dependencies and zero warnings.
Composition receipt and all unique bake/log/package evidence are retained.
Arcade's 5,294,172-receiver and Lab's 758,457-receiver preparation logs are useful
for the later compiler review, but overlapped builds/captures and are not clean
compiler-performance baselines. No compiler speedup is claimed.

Native execution is Windows/DX12 only. Metal/Vulkan, low-memory hardware,
arbitrary imported extreme rigs, extended residency and full queue-wide
regression/capacity claims remain unverified here. The mirror view exercises
the real reflection pass; it does not prove a moving NPC visible only inside a
mirror frustum. CPU sockets/collision retain their established paths; exact GPU
vertex checks add geometry confidence without replacing those platform tests.
Blood's previously documented flat pigment/reflection/transparency limits remain.
Quality/load cycles check stale data and validation errors, not a long-session
memory leak claim. HAL allocations are partial, not exact whole-process VRAM.

### Media and queue handoff

`Places-GPU-rendering-montage.mp4` combines preserved baseline lab samples,
accepted GPU lab samples and walking/ragdoll blood validation, with independently
measured lab savings in captions. 1339819 bytes, 23.625 seconds,
silent H.264, 1280×720, 8 fps timestamped native samples; decode verified.
SHA-256 `c301e1e5b693e23c26fe70508a95076a220f06eedb49a7eee9dbedffb8958d81`. The separate before clip, all original frames,
raw timing CSVs, traces, scripts and receipts remain outside target. Capture and
encoding are separate from final timing; these are short sampled videos, not
high-framerate recordings. Both clips are saved to Library; identities are in
`library-transfer.json`. Windows lacks the helper's local extended-attribute
API, so the sidecar retains the returned version mapping. No executable bundle
is uploaded.

Writer release, final commit/remote SHA, clean status and retained artifacts are
recorded in `gpu-rendering/ownership-handoff.json`. The next phase is
loading/memory/storage/compiler efficiency, followed by integrated performance /
regression, general polish, the dedicated full map-compiler and lighting review,
The Shape of an Exit second-to-last, then final verification/report/reconciliation.
The dedicated review must inspect parallel worker utilization, GPU bake
feasibility, redundant bounces/rays/repeated calculations, caching,
allocations/copies and synchronization, with preserved quality and matched
per-stage timings. This phase does not start those items. SYSTEM-only awake
helper PID 18136 remains; the queue stop file is not created. No CI wait, release,
main merge, security change or version bump.

## Loading and memory ownership pass — 23 October 2026

This focused Windows pass implements incremental preparation/install work and
real admission/eviction policy for #27–29 and the granular-loading item in #31.
It does not complete the whole resource-limit audit, storage/compiler work or
all acceptance criteria in those issues. The previous GPU work and its worsened
frame-time tails remain as reported above. No quality reduction, map-specific
exception, new map bake, package change or asset change was used here.

### Provenance and measurement boundaries

The matched baseline is clean, remote-verified staging commit
`b960f83962606778ddd21085b4640cf46a2961ac`. Baseline player SHA-256 is
`568f0de8c4aefbc9a0eaaf46c89351255733fb336756ffb4b8bfb5645f458f29`.
The accepted measurement cohort uses player
`30f500b91f4aea3d1a1fc384ddf2a481a9bd6d8449829f8504adf58a99a3bcd5`
and editor `3ad8a6be7c7bb2341945916d505285c4767b295c3e81f117512ba696085ab7f9`.
`frozen-source-accepted.json` records every changed source hash. These exact
binaries are retained in `accepted-cohort-bin/`; the baseline executables/DLLs
remain in `baseline-bin/`. A final cancellation-only amendment and its native
checks are identified separately below; the broad cohort is not relabeled as
measurements of that later binary.

All evidence paths in this section are relative to
`local/windows-validation-0.13.0/loading-memory/`. `matched-loading-memory.json`
contains the consolidated raw-derived numbers; before/accepted directories
retain original traces, action scripts, settings, process identities, CSVs,
25 ms sampled RSS/private commit, screenshots and diagnostics. The rejected
serial-texture candidate is retained as `after-*`, not mixed into acceptance.
The failed `before-lantern-cold` pilot used an invalid level ID and is excluded;
the measured map is `lantern_hollow`. Historical Arcade 8.063→2.456 s and Lantern
4.726→4.336 s figures are not this pass's matched baseline.

Native identity: Windows 10 build 19045, NVIDIA RTX 3070 Laptop GPU, DX12,
driver 617.14, optimized release. Player runs use High textures, Static/off
prepared lighting variant unless explicitly Full, Full reflections, 1280×720
physical drawable, VSync off and no frame limiter. The final surface/resource
logs confirm **Immediate**, after initial device creation logs FIFO; requested
settings alone were not used as proof. Swap/loop traces vary below the refresh
interval rather than plateauing at refresh. There is no FPS improvement claim
from these loading/diagnostic runs. Editor operation checks use its actual
1440×816 drawable/FIFO; they are not editor FPS comparisons.

Cold means a fresh application state; warm is a new process reusing that state.
Neither flushes OS or GPU caches. Within-process repeats are reported separately.
Authoritative load timings ran without captures, builds or bakes. Behavioral
captures and GPU audit runs are separate and supply no FPS result. The tiny
sample count does not support a statistical confidence interval.

### Implemented ownership and work scheduling

The existing coalescing single loader, bounded prepared-record LRU, weak live
identity reuse, page/prop upload staging and texture identities already solved
parts of the problem; they were retained. The loader now prepares physical item
worlds, world vertex packing/occlusion and the unchanged runtime water/caustic
field on the worker. Shared rigid-model validation lives with engine assets.
Fixed-before-movable collider insertion order is preserved. GPU calls remain on
the window thread, and the current world remains usable until atomic commit.
Graphics-only rebuilds reuse the resident physical world.

Archive decompression checks cancellation and reports actual decoded bytes every
64 KiB; typed binary reads check at crossed 64 KiB boundaries and collection
admission. Allocation uses checked arithmetic and fallible archive reservation.
One scoped CPU admission state is explicitly inherited by image/variant jobs.
A sticky refusal prevents fallback asset loading from hiding a memory rejection
and silently substituting lower-quality textures. Errors cancel/release pending
work and leave the displayed scene, held item and script state available.

World conversion advances in 256-vertex slices against a soft 4 ms quantum,
then uploads one index-width-bounded chunk and releases its packed CPU copy.
Textures use the existing parallel mip preparation in batches of four sources,
at most eight used semantic requests (normally at most about 43 MiB scratch for
High material sheets). Fixture faces follow individually; atlases and props
retain their existing bounded upload stages. Completion fences prevent unlimited
queued upload work. These are bounded work units, **not a hard 4 ms frame
promise**: shader/device startup, one GPU upload, final material/scene commit and
other format-capped CPU operations can still take longer. Arcade's largest
renderer call remains about 490 ms. Initial device/UI preparation remains an
approximately two-second startup component on this machine.

The loading overlay displays decoded MiB and accepts Escape during reading,
CPU preparation and GPU install. Superseded editor CPU requests share the same
cancellation state; superseded GPU installation is discarded while retaining
the previous displayed scene. No full-scene partial readiness is published:
first usable means the new scene was actually presented with gameplay/collision
ready.

### Operating policy inventory for this ownership boundary

These are operating charges/limits, separate from immutable format and hostile
input limits. They are not an allocator replacement or cross-hardware optimal
settings. Source names below are the source of truth.

| Pool | Units, scope and enforcement | Policy / failure |
|---|---|---|
| Load CPU admission, `engine/resources.rs` | Windows process private commit plus shared in-flight reservations, checked before archive/typed/image allocations and again before readiness; Linux/macOS use available RSS telemetry | 4096 MiB default; `PLACES_LOAD_MEMORY_MIB` clamped 128–8192 MiB. Refusal is sticky and preserves current map. CPU and GPU measures are never added as independent physical pools. |
| Map GPU admission, renderer `admit_prepared` | Old map + incoming map + deduplicated retained GPU texture handles; actual packed world/prop payloads, conservative alternate-state geometry, atlas bytes, 3× image charge and 128 MiB dynamic/staging reserve per world | 2048 MiB default; `PLACES_MAP_GPU_MIB` clamped 128–8192 MiB. Drop optional cache references first; refuse overlap if still too large. Live draw handles are never freed for admission. Shared old/new assets can be charged twice conservatively. |
| Prepared map records, game loading LRU | Actual retained immutable record charge, 3 entries / 192 MiB; weak identity list 8 | Existing LRU retained. Oversized maps stay live only while used and are not cached. Pressure/cancel/failure clears optional owners; explicit unload maintenance clears cache ownership. Cached records have no running scripts, AI or lighting updates. |
| Decoded catalog images, materials image cache | 256 optional images / 256 MiB, trim after resolution and insertion | Independently pinned images survive; their process memory remains charged. Content identity still shares equal assets. |
| GPU texture reuse, wgpu texture cache | Existing 256 entries / 256 MiB old-cache trimming at level start; active/new sheets may exceed that optional retention threshold | Old cache references are evictable; active worlds keep their Arcs. Aggregate map admission charges retained bytes rather than treating cache references as free. |
| Editor documents/scripts/history | Conservative owned JSON/string/typed-state charge summed across current, cached documents, saved copies and script histories; admission on open/edit | New 256 MiB aggregate charge. Only clean other documents with no history may be evicted. Unsaved work and undo/redo are pinned; refusal explains save/reopen recovery. Existing 64 revisions / 32 MiB per undo or redo stack retained. This is charged heap, not exact process RAM. |
| Editor model/appearance/preview assets | Current preview model paths plus placed/template appearance IDs | Prune unused appearances by identity, not Arc count, because cloned preview caches otherwise pin each other. The old displayed preview retains its own live handles. |
| Editor discovery/browser | Existing visible-row cache 8 and thumbnail cache 24 tiles (~1.76 MiB) | Already bounded; no duplicate rewrite. Native churn and deterministic discovery/mesh equality verified. |
| Archive/typed safety | Existing package source constants and shape/integrity checks | 8192 entries, 256 MiB ordinary entry, 1 GiB ordinary-map aggregate, 2 GiB validated-project aggregate; typed atlas exception 384 MiB + 64 KiB and its codec/shape bounds remain. Count/overflow/hash/path checks unchanged. No caps raised. |

The default 4 GiB CPU / 2 GiB GPU operating charges admit these representative
loads and reserve room for observed old/new overlap; the pressure runs below
prove enforcement. They are provisional Windows development defaults, not a
calibrated adaptive policy for every hardware tier. Admission cannot guarantee
recovery from every native/driver allocation or OS OOM. Missing/unreliable
telemetry and low-memory/unified-memory hardware need further capacity work.

### Matched loading and memory results

| Map | Process start → first usable, cold before → accepted | Warm before → accepted | Request → first usable, cold before → accepted |
|---|---:|---:|---:|
| Arcade | 5.662 → 5.981 s | 5.223 → 5.318 s | 3.743 → 3.801 s |
| Lantern Hollow | 4.799 → 5.053 s | 5.037 → 5.071 s | 3.507 → 3.691 s |
| Performance lab | 3.631 → 3.583 s | 3.646 → 3.568 s | 2.258 → 2.259 s |

Startup endpoints are mixed; this pass does **not** demonstrate a general load
speedup. The additional scheduling/admission work has a cost. A serial texture
experiment worsened the texture stage (Arcade 99.6→323.9 ms; Lantern
164.5→593.9 ms; lab 87.7→176.1 ms), so it was rejected and the bounded parallel
batch path restored. Its complete evidence is preserved.

| Within-process transition | Request → usable before → accepted | Longest renderer install call before → accepted |
|---|---:|---:|
| First Pool | 2.935 → 3.059 s | 2.246 → 0.257 s |
| Repeated Pool | 2.631 → 2.767 s | 2.175 → 0.202 s |
| Repeated Arcade | 1.240 → 1.435 s | 0.286 → 0.286 s |
| Repeated Home | 0.266 → 0.269 s | 0.074 → 0.069 s |
| Repeated lab | 0.126 → 0.133 s | 0.017 → 0.016 s |

Pool's existing roughly two-second runtime water preparation runs on the worker
instead of monopolizing a renderer install call. This is a responsiveness gain
with slightly slower total transitions in this cohort. It is not a lighting-bake
algorithm change. Final event-pump/commit costs remain and should be considered
in the later integrated regression pass.

| Map, cold run | Peak RSS before → accepted, MiB | Peak private commit before → accepted, MiB | Live RSS / private at usable + 1 s, before → accepted, MiB |
|---|---:|---:|---:|
| Arcade | 829.6 → 828.1 | 1251.8 → 1251.3 | 817.8 / 1239.3 → 827.6 / 1250.9 |
| Lantern Hollow | 1088.2 → 1074.6 | 1984.3 → 1966.2 | 1058.5 / 1984.1 → 1035.5 / 1966.1 |
| Performance lab | 383.3 → 384.3 | 531.6 → 534.0 | 383.3 / 531.6 → 384.3 / 534.0 |

Peaks are broadly unchanged. Live samples are taken while the scene is active;
the final process samples occur after shutdown cleanup and must not be called
steady live residency. HAL counters are partial GPU buffer/texture allocations,
not exact VRAM. For example cold Arcade's final reported buffers/textures are
262,406,144 / 172,949,504 bytes before and 262,995,968 / 184,352,768 after;
Lantern is 759,693,312 / 132,644,864 before and 760,676,352 / 155,320,320 after.
The retained texture working set grew in these runs, so no universal GPU-memory
saving is claimed. These counters must not be added to RSS/private commit.

The 30-transition accepted session lasts 183.39 s, including Home, Pool, Arcade,
Lantern, lab, Office and Beach. Sampled peaks are 1775.09 MiB RSS / 2716.02 MiB
private commit. Live lab after the first large-map cycle is 908.0 / 1838.3 MiB;
final lab is 970.1 / 1900.1 MiB. Lantern repeats are 1388.9 / 2580.8,
1394.5 / 2588.0, 1439.0 / 2625.6 and 1461.2 / 2649.0 MiB. This short campaign
shows retained working-set/allocator growth, not a proven flat steady state or
an unlimited-session leak-free result. Record/cache ownership is bounded and
retired resources release, but longer same-map-set residency, driver allocator
retention and integrated regressions remain work for combined verification.

### Cancellation, pressure, unloading and editor correctness

Three accepted cancel/reload flows cover archive reading, Arcade upload and Pool
upload, followed by two lab commits, an unsupported Full-variant failure and
successful Static recovery. Reading cancellation retires the worker in
44,257 µs in the retained trace. The extra Pool CPU-preparation cancellation
probe found a 1,409,919 µs drain after Escape. The final narrow amendment checks
cancellation inside runtime caustic receiver/source preparation and discards the
partial field before publication; it changes no noncancelled numerical work.
Final player SHA-256 is `2bbf0beb8de53bec6d7c5be8727f23adf62d4621bf8d86eb88c69fcc400ab414`;
editor is `59276c24dc196c59a453a507e4b73ee88f8647a7c1deb8f33662d61efe95db6e`. `frozen-source-final.json` proves the
only product-source difference from the broad cohort is this receiver/source
cancellation amendment. In `final-cancel-visibility/`, worker retirement is
**7,186 µs**, down from 1,409,919 µs, with Home preserved and lab recovery
successful. The separate final Pool native run completes with water/reflections,
closet door and flashlight intact; 65,120 receiver samples / 209,420 factors,
1,041,920 receiver bytes, 9 headers and no dropped sources match the accepted
noncancelled field. `final-native-acceptance.json` records the assertions.
Final screenshots were visually inspected; no bake algorithm was changed.

At 1024 MiB CPU budget, preparation refused when process commit 1,042,247,680 +
pending 33,554,432 + next allocation 4,614,598 exceeded 1,073,741,824 bytes.
At 384 MiB GPU budget, displayed 353,910,052 + incoming 488,869,884 exceeded
402,653,184 charged bytes. Both failures preserve Home and the actual held
cushion key `9223372036854775856`; neither discards the live scene to fit a budget.
At 850 MiB GPU budget, the same old/new pair fits 891,289,600 after optional cache
references are cleared; `gpu_cache=0` and Arcade commits successfully. This proves
admission/eviction behavior beyond recording resource numbers.

Explicit unload clears active prepared bytes, physical bodies, AI and Luau
modules/VM/frame calls to zero. Worker maintenance then drops the remaining
cache-only Pool record. By menu commit, old Home/Pool prepared record, build and
transport ownership counts are all zero. The new `places_menu` intentionally has
its own one script VM and five AI agents; these must not be confused with the
unloaded maps. Ordinary transitions retain only bounded immutable cached
records and intentionally shared catalog/renderer resources. Old scene state
continues only until the successful replacement commit, enabling rollback.

Editor native acceptance alternates preview modes 12 times, makes rapid edits
and undoes them, restoring the exact original document while leaving source
files untouched. Four images/five models remain in the stress preview cache.
Discovery digest and all three deterministic preview mesh digests match the
baseline. Edited document/history charge is 523,623 bytes, one revision / 40,423
serialized bytes; restored undo/redo history remains available (1,241,848 charged
bytes). Peak editor RSS/private is 421.9/574.0 → 409.1/567.6 MiB. Warm preview
11 request-to-ready is 201.8→234.7 ms, largest upload handler 14.1→14.0 ms; real
93-object document open is 10.77→10.94 ms, edit/undo 10.35→12.73 ms, cached switch
1.85→1.96 ms, clean save 212.46→209.94 ms. These include the additional admission
accounting and are mixed rather than a universal editor latency win.

Separate native Home pickup/carry/drop, Pool closet-door return/water/reflections/
flashlight, stress-map Luau spawn/despawn and Full Arcade checks pass. GPU skin
audits observe maximum position/normal error 0.000000238. No validation error,
panic or lost-device event is observed in accepted runs. Native captures are
retained with their action traces; no new speedup montage is claimed from mixed
whole-load timings and no executable bundle is uploaded.

### Disk distinction, final gates and precise next ownership

The unchanged nine-map validation collection archive is 134,910,682 bytes,
SHA-256 `5172d05c3152bf40842bf68f15ed0f4aba52f9902d7586ae68ee142e4b0e6f2a`;
unique uncompressed ZIP records total 465,657,130 bytes. Each isolated installed
state has the same 134,910,682 logical package bytes and zero files in its cache
directory. Lab archive is 721,782 bytes, SHA-256
`c77182184bd1ef3a33e23b08bbd16c1599f23adc771edac95042283a731b4b02`, with
6,024,202 logical record bytes; installed bytes are 721,782, cache bytes zero.
These are logical file lengths, not filesystem allocation/cluster measurements.
They are also not decoded RAM, GPU bytes, the full production collection's
footprint, or a new package-size reduction. Diagnostic logs are separate.

Typed records still materialize whole vectors. Shared-prop reconstruction still
holds segment bytes, reconstructed bytes and decoded output; other preparation
units retain their format-capped scratch. The next **storage/compiler** ownership
must profile and reduce these copies, compare codecs on actual records, review
lightmap/switch packing, serialization/staging and installed/temporary disk use,
while preserving every state, asset, source-recovery path and integrity limit.
Reuse the earlier accepted storage/provenance improvements; do not repeat them
or relabel their historical baseline as a fresh measurement. No compiler-stage
copying/serialization or bake algorithm optimization is claimed in this pass.

Final `cargo fmt --all --check`, strict
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace` (288 passed),
`cargo build --locked --release --workspace --all-features`, asset validator,
texture/prop source checks and `git diff --check` pass. Logs are
`clippy-cancel-final.log`, `tests-cancel-final.log`, `release-cancel-final.log`,
`assets-cancel-final.log`, `textures-cancel-final.log`, `props-cancel-final.log`
and `native-cancel-final.log`. Locked release completes in 6m 40s.
The execution transport disconnected before the amendment command started;
reconnection checks proved it had not run. The final native driver's initial
empty-process precheck returned shell status 1 before launching anything; its
log is preserved, the result handling corrected, and both checks then completed.
There is no pending permission or tool blocker.
Four new small tests have concrete regression value: shared reservation
boundary/overflow/drop plus sticky refusal; cancellation without parser cursor
advance; editor refusal preserving unsaved source/undo; and preview appearance
ownership preventing cloned caches from pinning an unused image. They cost no
GPU, asset discovery or shipped-map bake. The 288-test suite itself runs in
0.95 s after compilation. Texture source checks retain 100 existing catalog/
manifest warnings; the asset validator reports zero warnings. No lint level is
lowered and no broad lint suppression is introduced.

The full #27 data-limit inventory and adaptive supported-hardware policy remain
incomplete; this table covers the touched loading/editor ownership boundary.
Mac/Metal, Vulkan, low-memory machines, unified-memory calibration, controllable
native allocator failures and long-session steady state are not verified here.
The existing source shapes, package safety ceilings and collision/visual quality
remain enforced. The separate **full lighting-bake algorithm review** stays after
general polish: CPU parallelism, GPU feasibility, redundant rays/bounces/work,
cache validity and quality-equivalent matched measurements. It is not absorbed
by moving unchanged runtime caustic preparation to the loader.

After storage/compiler: combined performance/regression, general polish, the
dedicated compiler/light-calculation review, The Shape of an Exit second last,
then final verification/report/issues/cleanup. No later phase starts here.
Final commit/remote/clean status and writer release are recorded in
`ownership-handoff.json`; no CI wait, main merge, release or version bump.
SYSTEM-only awake helper PID 18136 is preserved and the queue stop file is absent.
All build and unique validation artifacts are retained.


## Coordinated storage, compression and compiler data handling — October 23

This focused Windows phase starts from remote-verified `fc497a6c44b6926fd642aa9e1a88bf39dca2675b`
on `0.13.0-staging`. It follows the loading/memory handoff and reviews the remaining
storage/compiler portions of #27–31. Existing content addressing, dependency-based reuse,
project spooling, exact prop factoring, compact lightmaps and counting-sink compression
selection were inspected and retained. Their earlier gains are not new results here.
Only this release report is updated among documentation files.

Evidence is under `local/windows-validation-0.13.0/storage-compiler/`, outside `target`.
Raw commands, binary hashes, process wall/CPU/working-set/private samples, delivered
progress boundaries, phase reports, input/output packages, native actions and PNGs remain
available. `frozen-source-final.json`, `codec-summary.json`, `largest-entry-seek.json`,
`stage-profile.json`, `byte-accounting.json`, `prepared-payload-parity.json` and
`native-acceptance.json` identify the actual comparisons. Tools and executable cohorts
are retained locally. Razer Blade 14, Ryzen 9 5900HX (8 cores / 16 logical processors),
16 GB RAM, Windows 10 build 19045, RTX 3070 Laptop / DX12; Rust 1.99.1 release builds.
Compiler runs use 12 workers. Samples are serialized with no concurrent build, bake or
capture during authoritative timings. “Cold” below means fresh process/output, with
no package reuse; OS file cache was not flushed. These are small matched campaigns,
not confidence intervals or supported-hardware calibration.

### Codec comparison and decision

The isolated Rust probe decodes **actual records**, then recompresses each independent
ZIP entry: Full stress lab (31,376,942 logical bytes), Full Arcade, and the complete
nine-level rendering collection (741,657,224 logical bytes). It does not compress an
existing ZIP as one opaque stream. All nine settings preserve identical decoded SHA-256
streams. Full reads, reverse-directory every-seventh-entry access, and an independent
seek to the largest atlas entry are measured. The latter avoids mistaking metadata-only
seeks for large-record access. Entries remain independently accessible; no solid archive
or cross-entry dictionary is introduced.

| Collection codec / level | Archive bytes | Encode s | Decode s | Largest-entry seek/decode s | Encode/decode peak RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| deflate 1 | 266,987,035 | 2.662 | 1.239 | 0.425 | 261.6 / 4.3 |
| deflate 3 | 224,953,624 | 4.762 | 1.232 | 0.431 | 261.6 / 4.3 |
| deflate 6 | 220,007,574 | 6.171 | 1.206 | 0.422 | 261.6 / 4.3 |
| deflate 9 | 217,699,563 | 17.049 | 1.265 | 0.447 | 261.6 / 4.3 |
| zstd 1 | 215,997,499 | 1.355 | 0.992 | 0.313 | 261.3 / 4.9 |
| zstd 3 | 212,247,940 | 3.023 | 1.022 | 0.319 | 262.0 / 6.4 |
| zstd 9 | 203,232,514 | 8.741 | 0.998 | 0.309 | 271.2 / 8.4 |
| bzip2 6 | 245,411,476 | 40.341 | 15.110 | 5.099 | 261.4 / 6.2 |
| xz 3 | 180,235,286 | 51.089 | 8.498 | 3.175 | 278.1 / 8.0 |

Encoder peaks include a whole decoded input record and process/allocator state; decoder
peaks use a 64 KiB output buffer. They are not isolated codec heap measurements. Detailed
CPU/private peaks and every single-level/stress result remain in the raw results. For
Full Arcade, Deflate 6/9 use 86,602,818/85,305,472 bytes and 2.315/7.080 s encoding;
Zstd 3/9 use 86,033,066/83,323,330 bytes and 1.226/3.171 s. For the Full stress map,
Deflate 6/9 use 7,345,807/7,211,461 bytes; Zstd 3/9 use 7,340,960/7,072,219 bytes.
Bzip2 is larger and slower on these inputs. XZ 3 saves substantially more distribution
bytes, but collection decoding rises to 8.498 s and encoding to 51.089 s.

**Retain the current Deflate defaults and migration level 9 in this phase.** Zstd 9
is a useful future candidate: collection size is 6.65% below Deflate 9 with about half
the encoding time, but full decode saves only 0.267 s in this probe, and no end-to-end
native load gain is established. The inspected zip 9.0.3 Zstd adapter constructs
`zstd::Decoder::with_buffer` without an explicit window-memory limit. Merely enabling
that feature would add decoder scratch outside the present reservation policy.
An output-byte cap does not bound an attacker-declared codec window. Adoption needs an
explicit window/scratch budget, hostile-frame tests, capability/version handling and
cross-platform acceptance. That work is deferred rather than shipping a feature-only
change or claiming an unmeasured loading gain. No project dependency or package format
changes here. The tested integrated alternatives use zip 9.0.3, zstd 0.14.3 /
Zstandard 1.5.7, bzip2 0.6.1 with libbz2-rs, and lzma-rust2 0.21.0. They require no
separate end-user executable. Only Windows was exercised. See the primary
[ZIP crate documentation](https://docs.rs/zip/9.0.3/zip/),
[Zstd stream decoder](https://docs.rs/zstd/0.14.3/zstd/stream/read/struct.Decoder.html),
and [LZMA Rust implementation](https://docs.rs/lzma-rust2/0.21.0/lzma_rust2/).

### Accepted lossless lifetime and packing changes

- Validation retires raw mesh/props/collision/navigation bytes after their typed checks.
  It retains only the model-name set required to validate the Static off state, instead
  of keeping all decoded main-state prop vertices while parsing that state and the atlas.
  Every original typed check still runs; this does not skip validation or remove states.
- Storage migration retires its runtime-admission world before allocating factoring
  buffers. Ordinary publication now streams owned, digest-pinned spool files through
  the existing bounded 64 KiB copy path instead of loading every spool into a new vector.
  Project publication retains its existing independent validation and larger limit.
- Compact lightmap packing borrows the already prepared irradiance/moment planes,
  eliminating a second full-atlas flattening allocation. The existing tile loop, precision
  bounds/exact fallback, alpha convention, atlas coordinates and state order remain
  identical. A cooperative check exists between plane pairs. There is no solver,
  resolution, sample, bounce, density or lighting-precision reduction.

| Matched operation | Process wall s, before -> final | CPU s, before -> final | Peak RSS MiB, before -> final | Sampled peak private MiB, before -> final |
| --- | ---: | ---: | ---: | ---: |
| Nine-level typed validation | 9.910 -> 10.006 | 9.719 -> 9.859 | 806.9 -> 768.9 | 802.2 -> 741.2 |
| Full stress typed validation | 0.296 -> 0.298 | 0.266 -> 0.281 | 84.8 -> 67.2 | 79.1 -> 61.5 |
| Nine-level storage migration | 60.073 -> 60.231 | 59.656 -> 59.781 | 806.9 -> 768.9 | 806.5 -> 756.0 |
| Standalone Full stress migration | 1.270 -> 1.323 | 1.203 -> 1.281 | 85.0 -> 66.3 | 79.1 -> 60.5 |
| New forced Lantern Static build | 38.209 -> 38.314 | 38.625 -> 38.625 | 1122.6 -> 1121.5 | 1433.8 -> 1434.4 |

Three fresh-process Arcade Full repacking pairs retain the **same 253,482,600-byte
encoded atlas** in every run. Median process wall is **7.660 -> 6.640 s** (13.3% lower)
and CPU **7.406 -> 6.594 s** (11.0% lower). Sampled peak private is about
**2,082 -> 1,954 MiB**, while peak RSS rises from about **1,820 -> 1,844 MiB**.
This is a private-commit/copy and operation-latency improvement, not a general RSS win.
The process operation includes archive read/decode and writing its output; it is not
a new lighting solve or a claim that the whole compiler is 13.3% faster. The small
Full stress packing pair is 0.595 -> 0.532 s with effectively unchanged peak memory.
Packed-page float materialization and other whole-record overlaps still dominate parts
of this operation. The repeated values and all six run identities are preserved.

The fresh Lantern pair remains essentially flat: no historical Lantern peak is
reconstructed or substituted. Its new phase wall times are navigation/collision
8.923 -> 8.753 s, prepare/encode Static 16.304 -> 16.561 s, prop sharing
10.373 -> 10.352 s, and final archive publication 2.225 -> 2.258 s. Prop sharing still
compares both exact layouts with the real compression writer and may reconstruct the
winning established candidate again to avoid holding both candidates simultaneously.
That measured CPU/memory trade-off remains. It is separate from ray/bounce calculations.

| Static pipeline / state | Cold force s | Warm force s | No-op s | Unrelated file s | Relevant light edit s | Observed cancelled run s |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Home | 0.818 -> 0.842 | 0.820 -> 0.820 | 0.106 -> 0.106 | 0.106 -> 0.106 | 0.840 -> 0.842 | 0.635 -> 0.630 |
| Two-level Janitor project | 0.631 -> 0.629 | 0.652 -> 0.633 | 0.106 -> 0.116 | 0.106 -> 0.112 | 0.566 -> 0.551 | 0.438 -> 0.434 |
| Performance lab | 0.631 -> 0.617 | 0.631 -> 0.631 | 0.086 -> 0.085 | 0.084 -> 0.085 | 0.631 -> 0.631 | 0.432 -> 0.430 |

These complete paths are effectively flat. No-op and unrelated-note edits preserve
verified package reuse; relevant edits rebuild. All prepared and semantic records for
the paired Home, Janitor, stress and Lantern builds match exactly, excluding only changed
compiler provenance. Progress-boundary process CPU/live-memory samples accompany precise
BuildReport phase wall times; delivered-line sampling is not exact per-function CPU or
heap attribution. Preserved Full GPU-phase bakes were reused, avoiding another Full
collection/stress lighting campaign.

Cancellation preserves the prior usable package and source; owned staging is removed.
The CLI standalone cancel/restart and all three incremental restarts pass, and a small
writer fixture covers cancellation during the streamed copy followed by successful
publication. Large migration cancellation remains coarse: total run is about 35.6 s,
with roughly 21 s after requesting cancellation during Lantern factoring before its
next boundary. That responsiveness limitation is retained explicitly. Normal and disk-
sampled migration repeats produce identical archives; no unowned artifacts are removed.

### Bytes, packing utilization and resource boundaries

The original complete rendering collection is **217,618,551 archive bytes**, with
**741,657,224 unique logical ZIP-record bytes** and **20 indexed recovery sources**.
The pre-existing optimizer yields **217,543,084 archive bytes** and **742,244,970 logical
record bytes** with both baseline and final code: the entire resulting archives are
byte-identical. Its 75,467-byte archive saving and raw-record increase are an existing
storage-selection trade-off, not a new saving from this change. Standalone migration
archives and the Full stress/Arcade re-encoded payloads are also byte-identical.
Metadata/scripts/source records retain their identities; disposable preview/history
state is not added to export. Removing the exact recovery sources merely because they
resemble normalized semantics would lose reimport fidelity and is rejected.

The Full Arcade atlas retains eight 1024-square pages and three groups (base plus two
independent circuits): **402,653,184 decoded half-plane bytes**. Its 71,766 existing
charts consume 7,852,086 / 8,388,608 texels including the actual per-kind gutters
(**93.60%**); props use their established one-texel gutter, not the metadata's two-texel
architectural gutter. The stress atlas retains two pages / one group and about 82.5%
gutter-inclusive occupancy. Existing MAXRECTS architecture packing plus disjoint prop
holes/coalescing already address substantial packing waste. No page reorder, sparse
switch format or chart rewrite is justified as a low-risk addition here; all original
switch contributions and precision guarantees remain intact.

Sampled temporary logical disk peaks are **959,788,054 bytes before and after** for
project migration; owned staging is empty afterwards. This is not installed storage.
For the isolated Home/Pool native states, installed archive logical length is
134,910,682 and NTFS allocation 134,914,048 bytes; cache is **zero files / zero bytes**
for both cohorts. The Full stress installed archive is 7,345,807 logical /
7,348,224 allocated bytes. The original Full collection itself occupies 217,620,480
allocated bytes. `GetFileInformationByHandleEx(FileStandardInfo)` measures allocation;
logical ZIP records, temporary spools, installed archive allocation, decoded RAM and
GPU charges are kept separate.

Native active prepared CPU charges are unchanged: Home **17,981,910**, Pool
**130,322,623**, Full stress **53,285,922 bytes**. Optional resource telemetry was absent
from the dedicated switch snapshots and is recorded as missing. Actual process peaks
remain in the run receipts. These are not exact VRAM measurements; no CPU/GPU allocation
sum or long-session residency improvement is inferred.

Touched constraints remain unchanged: standalone/project logical aggregate 1/2 GiB,
8,192 entries, 512 MiB binary records, 384 MiB plus header allowance for an atlas,
64 MiB metadata, 64 pages and four switch contributions, plus shape/count/overflow
checks and digest/CRC validation. Ordinary streaming retains the smaller aggregate
ceiling. Its metadata-only regression exceeds that aggregate with individually legal
records and fails before opening payload files. This is a touched-boundary review,
not completion of #27's whole-engine inventory or hardware-adaptive memory calibration.

### Native preservation, gates and ownership

Final code loads old packages, and both old/new compilers and native players accept
the newly editor-exported two-level project in Static and Full. Native Home picks up
key `9223372036854775856`, carries and drops it; Pool door/prop/water/reflection and
flashlight views pass; the Full performance lab remains correct. Static and Full
Arcade each exercise all four circuit combinations and restored states. Light-state
arrays match both cohorts, and twelve matched Arcade captures have **zero changed
RGB channels**. Animated Home/stress/water captures have small timing-dependent pixel
differences (Pool mean absolute channel difference below 0.003/255), with no visible
quality change. Actual presentation reaches **Immediate**, VSync and frame caps are off,
drawable **1280x720**. Captured action runs make no FPS-throughput gain claim. Startup-to-
usable remains mixed (Home 5.047 -> 4.053 s, Pool 6.055 -> 6.190 s, Full stress
3.812 -> 3.655 s); no general loading speedup is claimed.

The native editor exports/reimports the real two-level Janitor project and reopens it;
all three indexed source files match exactly. The original unindexed README remains
untouched in the source folder; it was never part of the recovery index. Initial setup
probes correctly refused a baseline executable missing its runtime DLL payload and a
Static request against a Full-only derived Arcade member. Identical retained DLLs were
restored locally and each variant was then tested using its corresponding preserved
package. A recovery audit initially included that unindexed README and was corrected to
the authoritative source index; no successful product operation was repeated to hide it.
All unsuccessful setup/audit logs remain with the accepted evidence.

`cargo fmt --all --check`, strict all-target/all-feature workspace Clippy,
`cargo test --workspace` (**290 passed**, suite runtime 0.94 s), locked all-feature
release build (**6m 33s**), asset validator (zero warnings), texture source checks
(100 existing catalog/manifest warnings), prop checks and `git diff --check` pass.
Two narrow regressions add essential coverage: exact borrowed-plane bytes across tile
and switch boundaries with malformed-plane rejection; and ordinary streamed publication
aggregate bounds, corruption, cancellation and successful retry. They use under 20 KiB
and 128 KiB fixtures, with no assets, GPU or shipped-map bake in routine tests.
Final logs use `*-final-strong-bound.log`, `*-final.log`, `final-checks-verified.log`,
`native-audit-verified.log`, `restart-verified.log` and `preservation-parity.log`.

A useful Pool acceptance PNG is saved to Library as `water-reflections.png`
(`libfile_ef909bded5c4819180b2b376c857b4b6`, version 0). Its local identity is retained in
`library-transfer.json`: the supported metadata helper cannot call `os.setxattr` on
this Windows Python runtime. The Library save succeeded; no access/security workaround
or executable upload occurred. All raw media and build artifacts remain local.

Deferred: bounded-window/versioned codec adoption; streaming typed shared-prop decoding
(segment + reconstructed-record + typed-output overlap remains); broader sparse/switch
layout changes; exact per-function allocation attribution and other hardware/backends.
The earlier worsened mannequin/menu-ragdoll tails and 30-transition memory drift remain
unresolved. The dedicated full lighting-calculation review still follows general polish
and covers lighting math, CPU parallelism, GPU bake feasibility and redundant rays/
bounces/calculations. This phase does not start it. After ownership release, the next
queue item is combined performance/regression, then general polish, that lighting review,
The Shape of an Exit second last, and final verification/report/issues/cleanup.
No CI wait, main merge, release or version bump. The final commit, exact remote SHA,
clean status and writer release are recorded in `ownership-handoff.json`. SYSTEM-only
awake helper PID 18136 and all build/unique evidence artifacts are retained; the queue
stop file is not created.

## Combined performance and regression sweep — 23 October 2026

This pass starts from clean, remote-verified `00f4966ba46beb583ab9558638be7a38c203f7c9`
on `0.13.0-staging`. It retains a measured editor history improvement and better
bounded resource instrumentation. Two engine experiments were rejected after matched
measurements: combining compute deformation with the first view encoder, and dispatching
small, expensive character-lighting batches through the existing worker pool. Production
renderer submission and character scheduling are restored exactly to the starting source.
No general FPS improvement or universal tail-regression fix is claimed by this pass.

Canonical receipts and the final commit/remote/ownership identities are under
`local/windows-validation-0.13.0/performance-regression/`. The original complete working
evidence remains in `C:\Users\Connor Dawkins\Documents\Codex\2026-10-23\task-2\sweep-evidence`.
The evidence index identifies retained package inputs, original locations and copied
receipts; failed setups, rejected sources and executable cohorts remain available.

### Measurement contract and profiling decisions

The same Razer Blade 14 / Ryzen 9 5900HX / 16 GB / RTX 3070 Laptop host runs Windows 10
build 19045 and DX12. Native throughput runs are uncapped, VSync off, actual **Immediate**
presentation and **1280x720** drawable verified from renderer/HAL observations, with no
missed ready presentations. Fast cases exceed the 165 Hz desktop reference. Five-second
steady windows use **actual ready-presentation timestamps**, aligned one-to-one with CSV
records; startup/install cadence is not summed into a second startup charge. GPU hardware
samples are deduplicated by sample id. Pass elapsed spans include scheduling gaps and
are not GPU utilization. CPU update, render submission, presentation and GPU spans are
reported separately. Builds, bakes, capture and encoding do not run during throughput
windows. Captured action runs and videos make no throughput claim.

Seven detailed scene profiles, five one-worker controls, fifteen baseline timing repeats,
42 same-build compute-submission controls, 36 scheduling controls, twelve two/four-worker
controls and six final scheduling repeats precede the decisions. The real editor-supported
12-mannequin/Luau lab, Full lab, Home, Pool closet, blood, menu events/ragdolls, Full Arcade,
mirror, fog overlap/clear and six-guest scenarios supply representative content.

The receiver-plan/key calculation itself measures approximately **0.001 ms** in the Full
lab and **0.03 ms** in the ragdoll menu. Rewriting that cache check is not justified. Actual
moving-rig spatial resampling is the meaningful work. One worker slows the lab and Pool
CPU paths. Neither observation supports a replacement job system or room/portal architecture.

The combined-encoder experiment is rejected: among other inconsistent means, median p95
menu time changes **5.283 -> 5.590 ms** and blood **2.409 -> 3.086 ms**. The separate submission
control is in the same executable. The experiment's source and binary are preserved;
`renderer.rs` has no retained change. There is no blood-specific fallback exception.

The scheduling probe uses prior exact visibility-query counts only as a cost hint, retaining
all samples and cache validation. Seven moving menu rigs each perform roughly 300 queries
while the existing count-only rule stays inline below eight dirty receivers. This reduces
CPU update, but the final repeated end-to-end result does not justify shipping it:

| Final same-build menu control, median of three runs | Existing policy | Cost-aware probe |
| --- | ---: | ---: |
| CPU update mean | 2.046 ms | 1.558 ms |
| CPU render submission mean | 0.915 ms | 0.959 ms |
| Presentation mean | 0.562 ms | 1.012 ms |
| Complete loop mean | 3.660 ms | 3.709 ms |
| Complete loop p95 | 5.818 ms | 6.249 ms |
| Complete loop p99 | 7.851 ms | 7.681 ms |

An independent earlier set also worsens p95 **5.471 -> 5.777 ms**, despite improving CPU
update **2.036 -> 1.510 ms**. Two workers increase menu CPU update **1.536 -> 1.810 ms** and
p99 **7.163 -> 7.610 ms**. GPU/presentation costs shift as CPU work changes; these results
do not establish a driver-level cause. The cost-aware source/accessor are removed and
all probe evidence retained. This is the last tail-tuning repetition in this pass.

Fresh unchanged-baseline repeats put the historical single-run differences in context:

| Case | Loop mean, median | p95, median | p95 range across three runs | p99, median |
| --- | ---: | ---: | ---: | ---: |
| Menu ragdolls | 3.748 ms | 6.084 ms | 5.556–6.183 ms | 7.985 ms |
| Blood | 1.706 ms | 2.510 ms | 2.420–2.526 ms | 3.573 ms |
| Full mannequin lab | 3.732 ms | 5.910 ms | 5.684–6.176 ms | 6.591 ms |
| Static mannequin lab | 3.365 ms | 5.789 ms | 4.968–5.822 ms | 6.372 ms |
| Pool closet | 2.241 ms | 3.949 ms | 3.931–4.268 ms | 4.455 ms |

These are current baselines, not improvements attributed to this commit. The former blood
tail increase is not reproduced consistently here; smaller lab/Pool differences are noisy.
The historical GPU-phase tail concern remains an uncertainty rather than a falsely closed
regression. Current evidence rejects the two proposed changes and preserves existing quality.

### Retained improvement: immutable editor history accounting

Every retained JSON history revision is immutable. Previously, each admission traversed
every retained tree to estimate heap use. A revision now caches its owned heap charge
after the actual move/clone. Admission sums those small records and also charges deque
capacity. Undo/redo still clones and charges the allocation actually retained; no stale
charge is transferred between differently sized allocations.

Three alternating before/after probes perform 64 edits and exact undo/redo on identical
isolated source inputs. Medians of mean latency for the final sixteen edits are:

| Project / objects | Before | Retained fix | Reduction |
| --- | ---: | ---: | ---: |
| Performance lab / 117 | 0.862 ms | 0.432 ms | 49.8% |
| Collection entry / 85 | 6.374 ms | 5.498 ms | 13.7% |
| Lantern Hollow / 1,270 | 60.332 ms | 48.035 ms | 20.4% |

First-eight latency is essentially unchanged; the saved work grows with retained history.
Full undo/redo cost is broadly unchanged. All 64 admitted revisions survive and exact source
restoration passes. Existing **64 revisions / 32 MiB per stack** and **256 MiB aggregate
document admission** remain; unsaved work and needed histories are preserved. The focused
80-tiny-revision regression verifies retained charges through trimming, clone capacity,
undo/redo and redo invalidation, using under 64 KiB and no assets, GPU or map bake.

Native small/large editor sessions exercise twelve environment-preview toggles, twelve
rapid edits/undos that cancel or supersede obsolete work, exact restore, Safety and source
preservation. Model/image caches settle to the same content counts rather than growing
with toggles: **5/4** in the lab and **63/19** in Lantern. Native peak RSS changes
**444.9 -> 446.3 MiB** for the lab and **1,839.9 -> 1,844.3 MiB** for Lantern; private
peaks are **593.5 -> 602.2 MiB** and **3,353.4 -> 3,363.9 MiB**, respectively. These
short-session peaks establish no memory saving. Large preview preparation still takes seconds; one observed upload handler
is about 139 ms. This pass does not claim a preview preparation or upload-latency win.
Those remain measured responsiveness leads, distinct from the retained history fix.

The real authoring workflow also previews/cancels/applies/undoes an open ceiling, edits a
local floor offset, saves/reopens, exports/validates Static and playtests the edited Luau
lab. A separate isolated intentional duplicate-wall counterpart exercises scoped ignore,
exact undo, redo, save/reopen, export/reimport and byte-exact indexed source recovery.
The ignore changes no geometry. Production map sources/assets remain untouched. Setup
failures involving popup selection and absent eligible warnings are preserved; successful
reviewed runs use explicit controls and an eligible placement finding.

### Independent Home, Relay and cross-feature preservation

Home cushion carry is tested separately from books/bed contact and the earlier Pool
props/water fix. Three uncaptured repeats per cohort cover idle, held/placed, interaction
and recovery phases. These probe-build observations do not establish a Home optimization:

| Scenario / phase | Baseline loop mean | Probe loop mean | Baseline/probe CPU update |
| --- | ---: | ---: | ---: |
| Cushion idle | 2.206 ms | 2.192 ms | 0.355 / 0.356 ms |
| Cushion held | 2.274 ms | 2.266 ms | 0.456 / 0.440 ms |
| Cushion moving | 1.319 ms | 1.318 ms | 0.404 / 0.406 ms |
| Cushion recovered | 1.292 ms | 1.299 ms | 0.325 / 0.331 ms |
| Two books, idle | 2.602 ms | 2.651 ms | 0.376 / 0.384 ms |
| Two books, placed | 2.638 ms | 2.676 ms | 0.437 / 0.416 ms |
| Bed contact/jumps | 2.124 ms | 2.161 ms | 0.436 / 0.437 ms |
| Stepped off/recovered | 1.731 ms | 1.757 ms | 0.375 / 0.367 ms |

Full mean/tail CPU/GPU component tables are retained in `home-interactions-summary.json`.
Cushion capture states prove sustained hold and ordinary drop, with at most one active
body. Books/bed runs activate two bodies and show grounded contact on the bed, ordinary
jumps and stepping off, with zero player resets. A first books setup crossed the room
boundary and reset before interaction; it is excluded. A side approach jumps about
**0.533 m**, below the **0.55 m** bed surface. The accepted contact fixture explicitly starts
above the bed before normal physics/jumps/separation; it does not claim an ordinary
floor-to-bed jump. Existing Portal-style movement and jump height are unchanged. That
approach sensitivity remains a movement/usability lead.

An independent native first-contact case proves the original dormant cushion activates
when the player lands on it and permits ordinary separation without reset; permanent fixed
instances remain unchanged. The severe reported Home slowdown is not reproduced on this
host. No special-case performance branch or reduced collision/visual quality is added.

Relay stairs and ramp jump checks and the crouch passage have zero resets/fall-through.
All **17 deterministic movement replays are byte-identical** to the published controller
baseline. The integrated scenes pass **1,181 lighting checks** and **34 GPU skinning oracles**
with position/normal error below 0.0001. Luau visitor counts return **13 -> 12 -> 13 -> 12**,
with one active script module and empty diagnostics. Six guests settle at nine characters;
Home pickup/drop, Pool closet door, water/reflections/flashlight, menu events, Arcade,
mirror and overlapping/clear fog remain correct. Production-build smoke checks repeat the
shared paths after restoring scheduling. Pressure refusal, cancellation/reload and unload
checks preserve the displayed old world until safe replacement and retire old ownership.

### Extended resource ownership and measurement attribution

The original 40-install baseline increases as new maps prime renderer/allocator caches.
The 100-transition, 101-install campaign then repeats the same ten maps per group:
Pool, Arcade, Lantern Hollow, performance lab, Office, Winter, Beach, Aero, Hub and Home.
Named capture-free checkpoints preserve the existing **128-action / 64 KiB script /
60,000 ms per-step** bounds while sequencing a longer campaign.

The first 100-transition run has sampled peaks of **1,905,860,608 RSS / 2,918,371,328 private bytes**.
Same-Home private memory changes from about **1,819 MiB** after the first group to
**1,882 MiB**, with the last forty transitions within about **7 MiB**. RSS still increases
approximately **8 MiB** over those last forty transitions. Handles remain roughly 573–583;
the active script count is one, with sub-MiB VM bytes fluctuating with collection. Active
prepared Home CPU charge stays **17,981,910 bytes**. Retired weak probes are bounded to 32;
evicted prepared/record/transport owners disappear, with at most the two permitted recent
cache entries retaining ownership. This supports bounded live ownership, not proof that
all allocator or driver allocations have been attributed.

Benchmarking itself retained up to **120,000 frame records** in addition to CSV. The new
opt-in `PLACES_BENCH_STREAM_ONLY=1` retains the bounded raw CSV on disk without that
duplicate summary vector, exposes exact record count/capacity/record-size/bytes, and keeps
the ordinary fallback if CSV cannot open or fails. Both CSV and record count retain the
same cap. A finite request longer than the cap now reaches its endpoint instead of hanging
after retention stops. The two-row seeded-boundary regression covers that behavior without
a long run. Normal gameplay benchmarking remains disabled and allocates no such records.

The retained production build completes another **100 transitions / 101 installs in 598.1 s** with streamed telemetry. Sampled peaks are **1,894,899,712 RSS / 2,898,374,656 private bytes**.

| Same-Home checkpoint | RSS MiB | Private MiB | Handles | Luau VM bytes |
| --- | ---: | ---: | ---: | ---: |
| group-0 | 894.3 | 1821.2 | 581 | 583,296 |
| group-4 | 925.7 | 1853.8 | 575 | 583,296 |
| group-5 | 927.1 | 1856.6 | 577 | 566,936 |
| group-9 | 931.9 | 1858.8 | 575 | 583,296 |
| settled | 931.9 | 1858.8 | 575 | 599,656 |

Across the last forty transitions plus settling, private memory spans **1854.5–1858.8 MiB**; RSS changes **+4.8 MiB**. Every checkpoint reports **zero retained benchmark records/bytes**, a **168-byte record size**, the unchanged **120,000-row cap**, and an active CSV stream. The last recorded count is 120,000; resource/ownership traces continue after that data cap.

Final Home HAL observations are **291 buffers / 103,874,560 reported buffer bytes**, **240 textures / 345,505,792 reported texture bytes**, **56 render pipelines**, and **217 persistent texture-cache entries**. These are partial resource facts, not driver allocation totals or exact VRAM. Active Home prepared CPU bytes, one current script module, bounded retired probes and retired ownership checks remain unchanged. The retained-record copy therefore does not explain all process residency; remaining page/allocator/driver attribution is unresolved. The observed late trend is reported without claiming an unlimited-session proof or normal-game RAM saving.

A 67-second preliminary streamed run was closed cleanly when the scheduling probe was rejected; it is retained as partial evidence and excluded from this final campaign. No build, bake, video capture or encoder overlaps the accepted resource process.


RSS/private memory and partial HAL buffer/texture byte counts are distinct. They are never
summed as independent physical pools or relabeled exact VRAM. GPU/driver reservations,
allocator high-water reuse and remaining uninstrumented ownership are not proven away by
successful cleanup checks. No live resources, unsaved work or history are discarded to
make a graph lower. Startup-to-usable remains mixed as already reported; this pass does
not claim every map loads faster or change the full lighting bake algorithm.

### Issue reconciliation, gates, media and remaining scope

| Issue group | Evidence and disposition |
| --- | --- |
| #27 repeated work, scheduling, tails, lifetimes | History accounting fixed; receiver-plan rewrite has no measured value; compute/scheduling probes rejected; live ownership and bounded diagnostic retention measured. Historical tail uncertainty and complete hardware-adaptive limit inventory remain open. |
| #27 Home/frozen props and Relay collision | Independent Home/contact and zero-reset Relay checks above; published movement replays preserved. Floor-to-bed approach limitation is explicit. Earlier Pool/contact/controller fixes remain intact. |
| #28 script/AI/spawn/menu/transition work | Real Luau expiry, guests, menu events, ownership retirement and refusal/cancellation validated. Earlier actor animation ownership, essential-supply well recovery, release/feedback, audio authoring/control/lifecycle and packaged live-menu work retain their canonical acceptance; these are prior completions, not new fixes here. Further general polish stays in its later phase. |
| #29 editor churn/author intent | Measured history fix, small/large native churn/cancel/undo, scoped ignore/recovery and ceiling/floor workflows pass. Prior structural floor/Annex-ceiling safeguards, colored XYZ labels and reversible axis dragging retain their earlier canonical completion evidence. Current Windows Relay checks reuse the preserved diagnostic movement package; a new complete Annex export campaign is not claimed. Large preview preparation/upload latency remains a measured lead. |
| #30 compiler/storage residuals | Prior actual-record codec, packing, installed allocation and ownership evidence remains authoritative. The missing historical Lantern peak and exhaustive per-stage copy/allocation attribution are still unavailable; native editor export/playtest is extended here without a fresh full-map bake campaign. |
| #31 larger projects | Existing Windows GPU skinning, conservative visibility, granular admission/loading and packing work retained. No new room/portal architecture or job system is justified. Codec adoption remains deferred until explicit decoder-window bounds and compatibility are resolved. |

Final retained-code `cargo fmt --all --check`, strict workspace/all-target/all-feature Clippy, `cargo test --workspace` (**292 passed**, suite runtime **0.91 s**), locked all-feature workspace release (**14.42 min**, one compile job), asset validation (zero warnings), prop checks, texture checks (140 textures, 100 existing catalog/manifest warnings), fifteen benchmark-accounting regressions and `git diff --check` pass. A parallel test compile initially hit Windows paging/commit error 1455; the metadata remained valid and the one-job retry passed. No paging/security setting or build artifact was changed or removed.

The final audit covers **18 integrated/production native runs**, **1,914 lighting oracles**, **58 skinning oracles**, four native editor sessions, seventeen exact movement replays and both complete resource campaigns. Final identities and logs use `production-*`, `*-production-final.log` and `*-production-retry.log`; earlier candidate gates remain preserved under their original names. Only the small history and benchmark-cap regressions are added; neither invokes a map bake.

Four short before/after videos and the concise timestamped montage are kept outside
`target/`, with decode verification and SHA-256 receipts. Final footage uses the retained
production build; captions disclose the rejected scheduler and the measured editor saving.
The final montage is `evidence/media-final/Places-performance-regression-montage.mp4`: **23.5 s**, 1280x720 H.264/YUV420P at eight native timestamped samples per second, **1,478,110 bytes**, SHA-256 `41dd4a4eaba7b6b1b260a32ed00bcdcfb0e7a331768354f32e66010a26865a0b`. Four individual clips and all raw samples remain alongside it. Library item **libfile_2d93ff14dc5c8191959ff1a3a16fc267**, version **1**, preserves the original saved file identity. Transfer and failed local-metadata receipts are retained; no executable bundle is uploaded.

The original encoder path in an older task was inaccessible. A standard task-local
imageio-ffmpeg dependency ran through the normal execution approval path; no security/ACL
changes, access workaround or executable upload occurred. The supported Library save
succeeds; its metadata helper cannot call `os.setxattr` on this Windows runtime, so local
Library identity is retained in the transfer receipt instead of falsely claiming xattrs.

Metal/Vulkan GPU-skinning acceptance, exact VRAM, lower-memory/unified-memory calibration,
the complete cross-boundary limit inventory, residual preview latency and historical tail
uncertainty remain genuine limits. General polish, the dedicated full map-compiler lighting
algorithm review (CPU parallelism/GPU bake feasibility/redundant bounces and rays), The Shape
of an Exit second last, and final validation/report/issues/cleanup remain the later queue.
No later phase, main merge, release, version bump or CI wait occurs here. Build artifacts,
all unique evidence and SYSTEM-only awake helper PID 18136 are preserved; the queue stop
file is not created. Final clean remote verification and writer release are recorded in
the ownership handoff.


## General polish - native Windows, 2026-10-23

Completed the approved general polish pass from clean, remote-verified
`c28fedbb0cfd0481bd51c77f98ebc7fd6faf54c6` on `0.13.0-staging`. Changes follow
native inspection of the actual player and editor. Production map sources,
existing artwork, renderer/scheduling and Portal-style movement remain unchanged.

### Concrete fixes and preserved content

- Unsaved editor edits no longer hide the existing **preparing view** indicator.
  Native Lantern Hollow captures now show both states during an actual rebuild;
  ready state, exact undo and source preservation pass. Preview diagnostics now
  measure admission and installation setup separately from background preparation
  and the existing maximum upload-handler timer.
- The bake checker button reads **Check saved** without truncation. Empty catalog
  search identifies the actual **Cmd/Ctrl A** clear command. Material, entity,
  fixture and door previews/theme filtering were inspected in the native editor.
- Jun's `collection:skeleton_friend` entity lacked a preview record. One opaque
  **160x120 RGBA8** PNG is rendered from its unchanged shipped GLB with the canonical
  offline preview builder. The native entity picker now displays Jun. All **525**
  previous records remain exact; the index now contains **526** previews. No other
  image or model is regenerated, and no runtime texture generation is introduced.
- One Guest Too Many's actual ordinary-input victory exposed a truncated result
  sentence (ending at `The bir...`). The existing result panel now wraps its detail
  text. A final-production ordinary-input win displays the whole sentence, retains
  six guests/nine render actors, and has zero resets or script diagnostics. No
  mode script, follower identity, containment proof or input behavior changes.

The existing menu is preserved after native standard/wide/portrait inspection and
a **194-second, 96-capture** whole-bag review. All **seven** accepted complete events
occur, below the approved **15-event cap**. Script diagnostics remain empty; the
intentional ragdoll event reaches seven rigs, and the roach event reaches fourteen
render characters including the six-character baseline. Inspected parcel frames
retain palm contact and pickup timing. Edge-to-edge dark backing, continuous fade,
removed slogans and sparse entry/action/exit remain accepted; no additional event
or scene rebuild is warranted.

After Hours - Rooms Beyond is visually reviewed using its preserved Static package.
A native walk from the authored spawn goes around the showroom furniture, through
the held-crouch closet, scaled rooms, staircase, upper low room and archive, reaching
the authored finale at **(6.950, 5.100, 18.931)** with zero resets and no player/item/
actor pose actions. View orientation controls are diagnostic. This extends movement
review and preserves the earlier Full visual acceptance; it is not a new Full-lighting
campaign. Original furniture/lighting sources and their completed packages are kept.

### Home contact and current-source Relay coverage

The standing Home floor jump still reaches about **0.533 m**, below the **0.55 m**
bed. It stops at the edge without reset. Ordinary crouch jump from the floor reaches
the mattress, stands, contacts two real book bodies, jumps, settles and steps off
normally, with zero resets. Only the initial player placement is on the floor;
the two books use explicitly labeled diagnostic placement. This closes the prior
approach-coverage gap without raising jump height or changing the controller.
Separate final native runs retain real cushion pickup/carry/drop, original dormant
cushion activation on landing and ordinary separation. Two books wake and settle;
the unrelated dormant/fixed content remains. Pool door/water/flashlight and the
scripted mannequin/carry lab also pass. Captured runs make no throughput claim.

Relay Annex is exported through the real editor from current source, recovered
through its editable-import UI, and all manifest/level/Luau/thumbnail bytes match
the original. All three members pass integrity and `verify --require-current`.
The native bake checker shows **three valid / reuse prepared lighting** rows.
Actual **Build+Play** imports, runs a real player child and returns to editing.
The standard complete-export cache is a byte-identical copy of the completed
export in the isolated recovery root, using the compiler's existing documented
cache contract; no compiler code or lighting algorithm changes. Native current-
package stair and ramp runs dismiss opening guidance, advance **5.517 / 4.654 m**,
jump and land at the authored upper supports with zero resets. Corrected tower
ceilings and solid support geometry are preserved. All **17** controller replays
from the final production tools are byte-identical to the published baseline.

### Preview latency remains a measured lead

Serialized baseline/final native controls use the same 1,270-object level and
ordinary editor presentation policy, with no overlapping build, bake, game or
editor process. Both preserve source, cancel/supersede work and undo exactly.
These phase observations are not uncapped FPS comparisons or a latency improvement
claim. Final milliseconds are:

| Native preview | Worker CPU | Admission | Install setup | Maximum upload handler | Request to ready |
| --- | ---: | ---: | ---: | ---: | ---: |
| Inspect after edit | 5892.0 | 0.091 | 5.88 | 145.4 | 7867.1 |
| Authored environment | 14787.1 | 0.075 | 6.31 | 170.9 | 16663.2 |
| Inspect restored | 5853.0 | 0.072 | 6.08 | 144.9 | 7717.3 |

The earlier historical capture reaches **262.27 ms** in the upload handler and
**15,011.98 ms** in worker preparation; the prior 139 ms statement was a representative
sample, not an upper bound. Admission/setup timers cover their actual synchronous
handlers, with setup including object/framing and renderer installation setup.
They do not turn remaining request-to-ready time into claimed GPU utilization or
attribute all frame blocking. Large preview preparation still takes seconds and
an individual upload handler still blocks for roughly 145-171 ms in these controls.
The displayed feedback is improved; the expensive work is not claimed optimized.

### Gates, evidence and remaining limits

Rust/Cargo **1.99.1**, final formatting, strict workspace/all-target/all-feature
Clippy with `-D warnings`, **292 passing Rust tests** (zero failed/ignored, **0.91 s**
suite execution), and the locked all-feature workspace release build pass. The
production build takes **15m05s**, one compile job; a final complete-source gate
rechecks the later result-text fix. The new preview also passes asset validation
(**629 assets, zero warnings**), prop checks and texture checks (**140 textures,
100 existing warnings**). No test is added and no map bake enters Cargo/CI.

All unique attempts, source/binary/package identities, captures and receipts stay
outside `target` in `C:\Users\Connor Dawkins\Documents\Codex\2026-10-23\task-3\polish-evidence`.
`local/windows-validation-0.13.0/general-polish/evidence-index.json` locates the
complete retained tree. Failed harness controls are preserved and excluded:
legacy crouch taps, an incorrect requested guest speed, browser popup/undo
assumptions, initial Annex guidance interception, and overlapping preview controls.
The first cold recovered Build+Play exceeded its verification idle wait and was
cooperatively canceled; the complete export/recovery already succeeded and the
reviewed standard-cache Build+Play passes without rebaking. An isolated browser
fixture stopped at unsaved confirmation after one undo reversed automatic light
mounting; its complete raw document was captured before closing that owned process.
No original project or unique user work was lost or saved over.

The byte-exact native ending screenshot is saved as
**Places-0.13.0-polish-ending.png**, Library **libfile_7ca458df0c108191ad6a7f6803cf2d0c**,
version **0**, **7,291 bytes**, SHA-256
`3fc1d82648f52d4b66e31dcf492c45f2d4a21fbb1347f85be7189577fe3251f9`.
The supported save succeeds; Windows Python cannot attach local xattrs, so exact
Library identity is retained in its receipt. The previously used encoder directory
was not readable; a native PNG is delivered without an access workaround or
executable upload. Windows command-line process inspection through WMI was also
unavailable; stored launch identity and ordinary process name/start time identified
the owned verification editor. No denied read is retried or security setting changed.

Historical FPS-tail uncertainty, allocator/driver attribution, the **1854.5-1858.8 MiB**
late private-memory range / **+4.8 MiB** late RSS observation, preview latency and
Metal/Vulkan/hardware-calibration limits remain as previously recorded. Rejected
compute, scheduling and worker-count probes are not reintroduced. No new FPS or
RAM saving is claimed. The dedicated robust compiler/lighting review is next;
The Shape of an Exit remains second last, then final verification/report/issues/
cleanup. No CI wait, main merge, release, version bump or target cleanup occurs.
Awake helper PID **18136** and its queue ownership are retained; the stop file is
not created. Final remote/clean verification and writer release are in the new
ownership handoff.

## Full lighting solver review - native Windows, 2026-10-23

The dedicated solver review starts at clean, remote-verified
`d5a062866136b9ccc203d2c038d011b6583cca6e` on `0.13.0-staging`.
It retains lossless CPU changes with actual complete-bake reductions on three
unchanged shipped maps. No density, atlas resolution, samples, bounce count,
precision, source material, light contribution or convergence threshold changes.
The solver revision remains **1**; compiler/tool provenance still changes and
invalidates packages normally. No cache identity is patched to force reuse.

### Complete bakes and stage attribution

The same Ryzen 9 5900HX (8 cores / 16 threads), 16 GiB Windows laptop, RTX 3070
Laptop GPU, Rust/Cargo 1.99.1 and **12 CPU workers** run both frozen executable
cohorts. Full uses **3 taps per axis, 2 bounce orders, 64 gather samples**.
Each timed run uses a fresh process, isolated application state, fresh output and
`--force`; OS/driver caches are not purged. Two runs per cohort/map, including a
reverse-order repeat, complete without build, bake, game or capture contention.
These are forced complete application bakes, not cached repacks or OS-cold claims.
Whole-process wall time includes validation, geometry, solving, reflection capture
where authored, compression and publication. Medians of the two runs are:

| Real map | Before wall (s) | After wall (s) | Wall reduction | Before CPU (s) | After CPU (s) |
| --- | ---: | ---: | ---: | ---: | ---: |
| Home: rooms, props, switched lighting | 100.337 | 89.568 | **10.7%** | 1010.961 | 1011.492 |
| Pool: grouped lights, interior, water | 344.893 | 228.453 | **33.8%** | 3591.836 | 2499.305 |
| Beach: exterior, props, different geometry, water | 342.038 | 288.303 | **15.7%** | 3115.742 | 3107.211 |

The actual before/after pairs are Home **100.013/100.660 -> 89.328/89.808 s**,
Pool **344.606/345.179 -> 228.071/228.835 s**, and Beach
**345.098/338.977 -> 288.513/288.092 s**. Home and Beach mainly improve wall
time through parallelism; they do not demonstrate a comparable reduction in CPU
work. Pool also removes repeated geometric work, reducing CPU work about **30.4%**.
Process CPU sums all threads; CPU/wall implies roughly **10.08 -> 11.29** effective
cores for Home, **10.41 -> 10.94** for Pool and **9.11 -> 10.78** for Beach.

Representative first-pair internal wall timers below sum the relevant base and
switch passes. They are not separate complete-bake speedups or additive to the
whole-bake table; indirect includes its nested cache/order work.

| Solver stage (s), before -> after | Home | Pool | Beach |
| --- | ---: | ---: | ---: |
| Receivers / coverage | 1.687 -> 1.680 | 7.889 -> 3.910 | 6.037 -> 6.110 |
| Direct illumination + visibility/transmission | 37.318 -> 37.020 | 114.196 -> 113.027 | 68.941 -> 68.450 |
| Indirect bounce traversal and gathering | 46.208 -> 45.616 | 184.769 -> 91.484 | 189.011 -> 184.043 |
| Indirect filtering, direct recombination | 11.786 -> 2.008 | 22.056 -> 3.857 | 61.358 -> 10.343 |

The full pipeline was traced through source/dependency validation and invalidation,
semantics/navigation/collision, legacy lighting, architecture/prop geometry and
charts, transport canonical hierarchy and ray BVH, receiver coverage, direct light,
opaque/alpha/water visibility, diffuse cache, each bounce gather and accumulation,
probe evaluation, surface filtering, authored fill, atlas encoding, reflection
capture and archive publication. Direct emitter integration and ray visibility are
fused in the production loop: its timer does not establish their separate CPU
shares. The isolated GPU probe below measures only an opaque any-hit kernel.

Geometry/chart construction and legacy lighting are subsecond here. Transport scene
and target preparation ranges about **0.2-1.6 s**; Beach navigation is **3.46 s**.
Candidate canonical plus ray hierarchy construction is about **12.7 ms** on Home
and **17.9 ms** on Beach. Diffuse spatial-cache builds take about **64-273 ms**;
per-order accumulation is about **35 ms** on Home and **119-229 ms** on Beach.
Probe evaluation is roughly **0.12-0.31 s**, authored fill **0.39-1.65 s**, atlas
assembly **0.07-0.22 s**, archive publication **0.28-0.52 s**. Beach's reflection
capture takes about **9 s** and is included in its complete bake. These small
stages do not justify a new large persistent cache or GPU transfer mechanism.

Native process counters distinguish peak working set, peak commit and sampled
private bytes. Home peak RSS is about **547 MiB** and commit **641 MiB** both ways;
Beach is about **2,280-2,287 MiB RSS / 2,924 MiB commit**. Pool before RSS spans
**698.5-828.9 MiB** and commit **1124.5-1254.1 MiB**; after RSS spans
**838.8-841.5 MiB**, commit **1253.6-1254.3 MiB**. The initial Pool pair alone
would misleadingly suggest a consistent 129 MiB commit increase: its baseline
repeat reaches the same commit peak. No consistent whole-process RAM improvement
is established. Worker/result storage can raise residency; this is not a RAM
optimization claim. Raw 250 ms CPU/private-memory samples carry delivered progress
stage labels; pipe/scheduling delay limits their stage attribution. Worker active
wall is not CPU time, sampled private peaks can miss transients, and neither is an
exact live-heap or GPU-VRAM breakdown.

### Retained work and scheduling findings

- The visibility-aware four-neighbour indirect filter was sequential. Receivers
  now use the existing bounded parallel executor with unchanged neighbour order,
  weights, arithmetic, direct recombination and indexed output order. All immutable
  field/cache data is shared. Per-worker interval scratch replaces repeated
  allocations in filtering and coverage refinement.
- The existing exact two-layer solver already shared geometry for a base plus one
  single-emitter circuit. It now accepts **all emitters of the first circuit**,
  including grouped emitters and maps with later circuits. Later circuits retain
  independent solves; live accumulator layers stay bounded at the existing two.
  Pool's three grouped emitters previously missed this path. Its nominal geometric
  gather slots halve **323,219,200 -> 161,609,600** while logical lighting-layer
  gather slots remain **323,219,200**. These are slots before geometric-horizon
  rejection, not a measured count of actual traced rays. Every contributing light
  and every authored bounce remains evaluated against its own layer values.
- The identical seeded local hemisphere sequence is computed once per bounce
  order instead of repeating random-number/trigonometric work per receiver.
  Transform/accumulation arithmetic remains identical. After a successful opaque
  visibility query, an immutable empty BLEND hierarchy proves unit transmission
  and avoids redundant normalization; water/alpha paths remain active otherwise.
- Existing workers dynamically claim contiguous chunks (normally 2048 for these
  large maps), with atomic job admission and indexed batch assembly. No nested
  solver scheduling or global per-ray accumulator lock was found. For Pool's long
  direct stage, worker active wall spans about **98.2-99.25 s** with 42-65 jobs and
  86k-133k outputs: variable work is balanced, rather than one worker owning a long
  fixed partition. Hoisting complete visibility or bounce results across changed
  material/light/geometry identities would be incorrect; no such cache is added.
- Full Home worker trials are **149.056 s / 579.188 CPU s** at 4 workers,
  **95.073 / 720.000** at 8, and **89.328-89.808 / 1008.625-1014.359** at 12.
  SMT gives diminishing wall returns while increasing CPU work. Two single 12-worker
  chunk trials, 256 and 1024, take **88.599 / 88.629 s**; this subsecond difference
  does not establish a broadly better default. Existing worker/chunk defaults stay.

Opt-in `PLACES_SOLVER_PROFILE` logs per-worker jobs/outputs/active wall and accepts
a bounded experimental chunk size **32-2048**; ordinary operation keeps its prior
chunk formula. New stage timers and the opt-in bounded real-scene ray snapshot make
the evidence reproducible. Ordered result buffers still require allocation/copying,
and gather data locality/pointer-heavy traversal remain possible leads. Wide SIMD
ray packets, a transfer matrix, all-circuit batching, a persistent thread pool and
cross-bake transport caches are unmeasured proposals, not shipped gains. No SIMD
precision relaxation, zero-energy threshold or approximate early convergence is
introduced. Runtime mesh transport scenes are already cached per mesh; they are
not a newly discovered every-frame rebuild. The editor's expensive geometry/CPU
preparation and separate upload blocking remain measured leads, without a preview
latency improvement claim from these solver changes.

### Measured GPU candidate, rejected for production

A standalone, bounded **wgpu 30.0.1 / WGSL** opaque any-hit BVH prototype runs on the
actual RTX 3070 Laptop through DX12. It uses **16,384 evenly distributed actual
receiver rays per map**, the real opaque geometry/hierarchy, half finite
receiver-to-emitter queries and half long hemisphere queries. MASK/BLEND/water faces
are explicitly excluded (Pool excludes 56 faces); this is not a complete lighting
solver or a water/alpha acceleration claim. Snapshots, Rust/WGSL source, lockfile,
binary, limits, transfer sizes and every mismatch are retained outside `target`.

| Opaque ray microbatch | CPU, 12 workers (ms) | Standalone device setup (ms) | Pack/upload/pipeline (ms) | First dispatch/copy/sync (ms) | Warm dispatch/copy/sync (ms) | Predicate mismatches |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Home | 1.545 | 2282.925 | 70.747 | 12.058 | 0.42-0.47 | 0 |
| Pool | 1.905 | 1028.750 | 30.311 | 11.322 | 0.59-0.66 | 151 |
| Beach | 1.127 | 1002.228 | 30.298 | 9.987 | 0.42-0.51 | 0 |

Warm GPU dispatch is faster for this kernel, but neither setup nor first readback
amortizes over one batch. A future implementation could share the compiler's
existing capture device; these standalone setup costs must not be treated as an
unavoidable cost of every possible GPU design. Pool has **118 false blockers and
33 missed blockers** against the current CPU primitive. Of 151 mismatches, 150 are
finite **unpadded endpoint** primitive queries and one is a hemisphere query;
these stress the raw any-hit API, not the production shadow-padding policy. They
are not 151 demonstrated full-bake lighting errors, and no GPU bake image is
produced. Exhaustive triangle precision controls reproduce **55** cases where
GPU agrees with the f32 oracle and CPU agrees with f64; other cases are not fully
attributed to precision alone. The current candidate fails exact primitive
equivalence, so its warm speed is insufficient for native solver acceptance.

The tested adapter does not expose `SHADER_F64`; the pinned wgpu feature documents
native f64 support for Vulkan, not DX12/Metal. A portable exact-predicate strategy,
canonical tie handling, alpha/transmission evaluation, device limits, transfer
residency and synchronization still require work and matched quality validation.
No proprietary end-user profiler/tool or GPU-only requirement is added. CPU solving
remains the safe path on every platform, including Mac; actual Metal/Vulkan/Mac
hardware and integrated/low-memory device limits are not validated here. This
rejects the measured prototype, not GPU acceleration in principle.

### Exactness, invalidation and native acceptance

All twelve representative force bakes retain identical decoded geometry, lightmaps,
probes, switch layers and other payload records. Source/settings/assets and tool
cohorts have recorded hashes. The comparator permits only the compiler-code and
tool hashes inside build-input provenance, and their verified manifest/fingerprint
references, to differ. Every other record is byte-exact, original package integrity
is verified, and tool hashes match the frozen binaries. No stored fingerprint or
package record is altered to pass the comparison. Smaller test_room,
material_transport, water_transmission and lighting_quality fixtures also match
exactly for both supported authoring variants, Static and Full. Legacy Medium
reading remains unchanged; no Medium authoring is reintroduced.

The isolated Static/Full incremental fixture has exact cold/warm output
(**11.066 / 11.072 s**). No-op, metadata, presentation and unrelated-note controls
take about **0.25 s**; metadata/presentation refresh only reusable prepared data.
Geometry, light, prop and material edits trigger fresh required work and each
matches an independent forced-clean archive. Real Home/Pool/Beach no-ops take
about **0.254 / 0.253 / 0.506 s**, `rebuilt=false`, preserving archive bytes.
These cached controls are not included as cold solver improvements. Cooperative
direct-stage cancellation returns **130**, preserves original source and old
published package exactly, removes owned staging and restarts to exact clean output.

The real Windows compiler validates complete nine-member project containers with
the selected Home/Pool/Beach measured records preserved unchanged. Every selected
member passes `verify --require-current` against its corresponding frozen compiler.
Other members are preserved dependencies and are not claimed newly baked/current.
Eight final native before/after view pairs match frame number, simulation time and
camera, and **every RGBA pixel is identical**: Home bedroom on/off and wall joins,
Pool service room on/off and actual water/transmission, Beach shoreline and
prop/geometry shadows. Full-frame originals are retained. No new smudges, seam
change, leaks or brightness shift are observed; this is equivalence to the current
baseline, not a claim to remove pre-existing visual artifacts. The old d5a player
also reads the new complete Pool package. Native capture is fixed-step visual
verification at 1280x720 with VSync off and no frame limiter; it is not an FPS
throughput comparison and supplies no new runtime FPS claim.

The actual native editor saves, exports, editable-reimports with byte-identical
manifest/source, launches Build+Play and returns to editing. The exported package
passes integrity/currentness and loads in the native player. A separate Save+Build
and **Check saved** capture shows **valid / reuse prepared lighting**, matching
dependency hashes and integrity. Original project/map sources stay unchanged.

### Gates, evidence and custody

Final formatting, strict `cargo clippy --workspace --all-targets --all-features --
-D warnings`, **294 passing Rust tests** (zero failed/ignored, **0.94 s** initial
suite execution), and `cargo build --locked --release --workspace --all-features`
pass. The full production rebuild takes about **14m23s**, one compile job; final
cached complete-source gates also pass. Two new small deterministic CPU tests have
concrete value beyond package coverage: grouped/multiple-circuit equivalence against
independent solves, exact serial/parallel filter/probe output through a hard divider
and tinted water, and cancellation without partial publication. Their 100-receiver
fixture uses under 1 MiB and milliseconds, no map assets/GPU/full shipped bake.
No full map bake enters default tests or CI. The relevant compiler/lightmap Python
suite passes **18 tests**; assets pass **629 / zero warnings**, props **389**,
textures **140 / 100 existing warnings**, and `git diff --check` passes.

The additional broad Python sweep is explicitly **not green**: 366 tests report
8 failures, 9 errors and 16 skips. A detached clean d5a checkout with frozen baseline
executables reproduces the **exact same 17 failure/error headers** and exit code.
They concern CRLF/historical hashes, stale Lantern counts, Unix-path/shell execution
expectations on Windows and a visual-CLI fixture. No relevant tools/tests/assets or
projects changed from that baseline. Both complete logs and the comparison are kept;
this review neither conceals the failures nor starts unrelated final-queue cleanup.

`local/windows-validation-0.13.0/lighting-solver/evidence-index.json` indexes all
source/binary/input identities, raw timings, CPU/memory samples, payload comparisons,
GPU source/results, invalidation/cancellation, native captures and gate logs in
`C:\Users\Connor Dawkins\Documents\Codex\2026-10-23\task-4\solver-evidence`.
All build and unique validation artifacts stay outside `target`; target is not
cleaned. The clean detached baseline at `...\task-4\baseline-source` is retained
for the parent's later cleanup; it is not another writer on staging.

Excluded setup attempts are retained: a standalone transition-bearing showcase
package opened the menu until complete project composition supplied its dependencies;
that owned menu process was closed. Initial native captures at adjacent simulation
frames are superseded by the eight exact pinned pairs. A strict package comparison
initially flagged only legitimate provenance changes; the verified provenance-aware
comparison resolves them. An unsupported Medium authoring request, a cancellation
observer on the wrong stream, missing asset-root validation and a nested-JSON editor
postprocessor are harness issues, with corrected accepted runs recorded separately.

The reviewed comparison is **Places-0.13.0-Lighting-review.png**, 1280x3180,
**1,320,181 bytes**, SHA-256
`d922caf01dc46724b10c5d80ed7b571c32d18b6186239bae0d0b682974f7ce26`, saved through
supported Library media upload as **libfile_173b7c26e24c81919ea181f2cd62c282**,
version **0**. Windows Python lacks local `os.setxattr`; the successful save receipt
and exact Library identity are retained instead. No executable bundle is uploaded.

The prior storage change's Arcade **7.660 -> 6.640 s REPACK** remains a packaging
result, not a solve gain. Zstd9 remains deferred pending decoder-window bounds;
historical Lantern peak memory remains missing. Exact per-function CPU/live heap,
GPU VRAM, broader platform/hardware calibration and residual direct/gather traversal
are remaining limits. No power/security setting is changed. SYSTEM-only awake helper
**PID 18136**, bounded expiry **2026-10-23 19:49:06 UTC**, and its queue custody are
preserved; no stop file is created. Final clean remote verification and writer
release are in the precise ownership handoff. The Shape of an Exit is second last,
then final verification/report/issues/cleanup; neither phase starts here. No CI wait,
main merge, release or version bump occurs.

## The Shape of an Exit: Windows acceptance (October 23)

The queued second-last phase implements [issue 25](https://github.com/csd113/Places/issues/25) in
`projects/shape-of-an-exit/`. One map links a low shutter, an elevated power switch,
a cabinet weight plate and a closing exit door to an enclosed exit vestibule.
Forty existing props, eight fixtures, stained office surfaces, filing storage,
noticeboards, desks, books and litter give each bay a purpose. The exit sign,
physical openings and interactable props carry the puzzle. No tutorial, hint
popup or authored guidance message is used; completion uses the normal win screen.

`player_form` and Luau `api.become(model?, scale?)` reuse shared gameplay actions.
Eligible props receive measured, bottom-anchored bodies with a square envelope
that contains every yaw. Static geometry, ceilings, floors, live doors and physical
items must clear before the visual and body commit. Held items prevent a shape
change until dropped. R returns to the ordinary human body after the same checks;
V changes the form view so large furniture cannot obscure a switch. Character
representations retain their existing human body contract. Ordinary human traction,
air control, jump, step and crouch parameters are unchanged. The physics proxy,
carrying clearance and door obstruction queries all receive the actual body width
and height; no map-specific collision exception was added.

Native paced recording exposed a rare general sweep defect: a start accepted
within the existing 100-micron contact tolerance could produce a negative SAT
entry, admit an inward step and then freeze later motion. The shared sweep now
blocks incoming contact-band motion and permits ordinary separating motion.
One small CPU regression covers three body widths at three door angles and keeps
deep invalid starts blocked. Together with the measured-envelope, low-header/atomic
return and live-item proxy regressions, four essential tests protect behavior
absent from the previous human-only coverage. No test or automatic CI bakes a map.

All geometry and final placement were saved through real native editor controls,
including the away-opening exit leaf, clear reset bays, floor/landing edges and a
finish volume overlapping the raised floor by 4 cm. Native export, editable reimport
and Build + Play pass. Manifest, level and Luau source bytes round-trip exactly.
The complete project package validates and verifies current against the final
compiler and asset root. It is `C:\Users\Connor Dawkins\Documents\Codex\2026-10-23\task-5\exit-evidence\contact-final-office.placesmap`; its hash and tool identities
are indexed locally. All **16 prepared lighting, prop, mesh, collision, probe and
navigation records** are byte-identical to the same polished map before the contact
fix. The prior solver and water/prop/ragdoll lighting optimization sources are kept.

The final ordinary route reaches the win screen and Enter restarts with human
body, locked records door, closed doors, cleared held state and restored mug.
It uses W/A/D, E, R, V and Space through normal handlers, plus native aim-only view
actions. Its action file contains no actor/item pose, direct actor dispatch,
console, fixture or outcome injection. Native focus, no noclip, no guidance,
body bounds and zero unintended resets are asserted. A second complete run is the
recorded video. Eight additional final native cases pass: human/crouch shutter
refusal and cup retreat; repeated return attempts and jumping under the header;
held pickup/drop and shape refusal; 12 chair/cup/human cycles; wrong-form and human
jump switch attempts; same-level reload followed by the cup route; cup/human ramp
and landing traversal with broad-body narrow-room refusal and R recovery; closing
doors stopping for human/cup/cabinet; all four wall resets; and rear-release/return-cup
reverse recovery. Focused cases label their diagnostic starting poses/direct dispatch
explicitly; those are not represented as the ordinary completion route.

The map's first throughput pass preceded final furnishing and visual polish.
Final capture-free runs used Full/high at **1280x720 physical drawable**, VSync off,
swap interval **0**, and actual **Immediate** presentation on DX12. Display refresh
is 165 Hz, while all medians exceed it. No build, bake, recording or other game ran
alongside these measurements. Recording's explicit pacing is excluded from FPS evidence.

| View | Before polish median FPS | Final median FPS | Final loop p99 ms |
| --- | ---: | ---: | ---: |
| reception | 572.64 | 545.94 | 5.724 |
| records | 597.51 | 567.63 | 4.657 |
| dispatch | 577.80 | 352.44 | 4.804 |
| exit | 452.63 | 444.27 | 6.008 |

No throughput gain is claimed. Dispatch was investigated in a separate repeated
campaign with the same pre-contact-fix binary and 1 Hz read-only GPU telemetry:
before-polish medians 576.34/565.36, polished 369.07/391.97 FPS, with 44 draws and
18,828 indices in both. GPU medians rose from 1.645/1.859 to 2.290/2.380 ms and clocks
varied substantially under the laptop's existing governor. This records the observed
render cost; it does not isolate a general engine regression or establish matched-clock
savings. No driver/power tuning or speculative renderer change was made.

Final strict Clippy, formatting, `cargo test --workspace` (**298 passed / zero failed /
zero ignored**) and locked release workspace/all-features build pass. Relevant Python
checks pass **21 tests** (the previous 18 compiler/lightmap tests plus three existing
native-regression-map tests). Assets pass **629 / zero warnings**, props **389**,
textures **140 / 100 existing warnings**. The broad Python suite is **not claimed
green**: the preceding clean-d5a comparison reproduced 17 failures/errors. That known
baseline is carried; this phase did not rerun the broad sweep or change Python sources.

The verified MP4 is **32.53 s**, 1280x720 H.264/AAC,
**887 native frames**, with all 887 decoded, actual game-output audio and zero audio
tap drops during the route. Its cadence follows measured capture wall time without
inserting frames. The original RGBA stream, PCM, per-frame timestamps and screenshots
remain outside `target`. All four small media files are confirmed in Library:

| Media | Confirmed Library ID | Version | Bytes |
| --- | --- | ---: | ---: |
| The-Shape-of-an-Exit-Gameplay.mp4 | `libfile_fcb6216ad6fc8191a8511f414b341b8c` | 0 | 7,949,469 |
| The-Shape-of-an-Exit-Cup.png | `libfile_8f6502a672b481919f61f23056d6cdb6` | 0 | 593,476 |
| The-Shape-of-an-Exit-Door.png | `libfile_b6faaba856dc8191b352e4ed7d815edf` | 0 | 753,835 |
| The-Shape-of-an-Exit-Office.png | `libfile_0eea136a8f5481919f6044d356ba6825` | 0 | 937,505 |

The prepared-upload Python helper could not discover its endpoint here; no helper
write began. Supported direct host-upload Library calls succeeded. Windows Python
lacks `os.setxattr`; exact successful receipts and local file hashes retain custody.
No executable bundle was uploaded. Retained excluded attempts include initial route
aim/timing corrections, a too-exact finish-volume floor boundary, one invalid terrain
fixture start, occupied evidence-directory names and an outcome-tuple postprocessor
mistake. The failed paced recording led to the shared contact fix; the final paced
recording and all native cases pass after it.

`local/windows-validation-0.13.0/shape-of-an-exit/evidence-index.json` indexes final
identities, gates, native controls/captures, raw performance/GPU logs and media in
`C:\Users\Connor Dawkins\Documents\Codex\2026-10-23\task-5\exit-evidence`. No unique evidence or build artifact was deleted and `target` was not cleaned.
Other platforms, matched GPU clocks, precise live heap/VRAM and the known broad
Python baseline remain outside this acceptance.

The new user request for a **real editor bake progress bar is explicitly pending**
for the final editor-fix phase. Existing compiler source/variant/stage/work events
currently reach footer text, not a bar. The exact inspection findings, relevant
files, truthful counted/indeterminate scope and cancellation/error/retry/publication
acceptance are in `local/windows-validation-0.13.0/shape-of-an-exit/bake-progress-handoff.json`.
It was deferred under the parent's explicit option to preserve this map's acceptance;
it is not reported as implemented or visibly validated.

The same bounded keep-awake mechanism was renewed: **PID 34744**, expiry
**2026-10-24 17:57:20 UTC**. The old helper was retired only after the replacement
confirmed its SYSTEM-required-only request. Display-required, power-plan and battery
protection settings remain unchanged; the queue's global stop file remains absent.
Final commit/remote/clean verification and writer release are in the ownership handoff.
Default-lineup/collection reconciliation and the final verification/report/issues/cleanup
phase are not started here. No CI wait, main merge, release or version bump occurs.


## Editor bake progress and queue handoff

Native Windows acceptance, 23 October 2026, from clean staging commit
`e731cea2e17985547f3b36dc8d2f9c7851b53eef`. This completes the authorized
**bake-progress phase**, not the final combined release gate. The user subsequently
queued four more sequential optimization passes and moved final verification and
cleanup after them. No later pass starts in this phase.

### Implemented progress and targeted acceptance

The editor footer draws a real **current-pass** bar from compiler work counters,
with the exact completed/total/unit, source, lighting variant and stage. Uncounted
stages use an animated track explicitly labelled **Indeterminate / uncounted stage**.
No whole-bake/export percentage or ETA is invented. A full pass track does not
finish the job: success still requires the compiler's publication report and
successful exit. Existing nonblocking polling, safe cancellation, atomic output
and save/recovery behavior are retained.

Scope is frozen when the job starts. Selected-level and stale-row builds say
**Current level: ID / other level outputs kept**; complete-project export,
validation and source recovery have distinct labels. Feedback stays above popups.
The normal toolbar Cancel is the only Cancel tab stop; a footer Cancel appears
when a popup clears toolbar hits. A duplicate-focus conflict was found and fixed
before the accepted native runs. No unrelated UI redesign is included.

The actual final release editor bakes real Home Static/Full data in an isolated
copy of the nine-member environment project. **23 counted native samples** cover
base/switched direct visibility, both shared bounce passes and indirect filtering.
The screenshots show the live counter and proportional track; they are not mocked
or replayed feedback. Grid/framing, Tab focus and a non-mutating search popup work
while baking. Ordinary/popup views each expose exactly one Cancel control.
In-flight cancellation preserves the previous package byte-for-byte. Retrying
from Home's stale row publishes only Home; all other sources and the preserved
Pool output remain exact. A real compiler error from an invalid unselected source
in this disposable project is reported, the source is restored exactly, and the
normal cached retry succeeds.

A transition-bearing per-level output is a prepared project member. Standalone
`validate` correctly refuses it without complete-project transition context; this
is retained as an expected scope refusal, not concealed as a pass or corruption.
The member verifies current. A complete nine-member diagnostic container replaces
only Home with these actual compiler records, preserving other dependencies.
Its integrity and Home currentness pass. Matching Full mesh, lighting, lightmap,
irradiance, collision and navigation records are byte-exact against the preserved
solver control. The control is Full-only; the editor also emits Static and shared
prop recipes. No identical archive/record-set or unverified prop-reconstruction
claim is made.

Native Shape export and editable recovery preserve manifest, level and Luau bytes
exactly. The export validates and verifies current. An initial cold recovered Play
process ended without a terminal receipt; no editor/compiler/player remained when
inspected. Its partial staging is retained and is **not** claimed passing. The
unfinished Play check is resumed through the normal cache path using the exact
completed export; cached Build+Play succeeds with an actual native child capture.
No source data, dependency hash or cache key is changed to obtain reuse.

The reviewed **Places-0.13.0-editor-bake-progress.png** is an unmodified native
1280x720 Full-bounce frame: **200,243 bytes**, SHA-256
`cb61c4ec89b176e18eb524185c572553efdd3cde088b3e5e2a1e8e65524f42ec`.
Library save is confirmed as **libfile_6f4a92e05d108191ac51f172e55ede4c**, version **0**,
file **file_00000000557481fb91d55ff1c9757bd1**. Raw frames/checkpoints stay outside `target`.
Windows Python lacks `os.setxattr`; local metadata application fails explicitly,
while the successful save receipt, exact IDs and hashes retain custody. There is
no duplicate upload and no executable upload.

### Gates and additional combined observations

Rust/Cargo **1.99.1**, formatting, strict
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace` (**298 passed**, zero failed/ignored), and
`cargo build --locked --release --workspace --all-features` pass for the frozen
implementation. Relevant Python checks pass **21 tests**; assets pass **629 / zero
warnings**, props **389**, textures **140 / 100 existing warnings**. No new Rust test,
full-map Cargo/CI bake or lint suppression is added. Engine, game, compiler/solver,
assets and authored project sources are unchanged from the preceding handoff;
456 source/build-input identities are frozen and rechecked.

The broad Python rerun is **not green**: **366 tests, 8 failures, 9 errors, 16 skips**.
Its exact 17 failure/error headers match the preserved clean d5a baseline, with
no new headers. Six errors need the unavailable Unix shell, two need symlink
privileges, and one uses unsupported subprocess SIGINT. Failures cover two CRLF
capacity fixtures, a historical hash, two Unix-path expectations, two stale Lantern
counts and the visual CLI fixture. Detached baseline evidence remains intact.

The extra combined work completed before the queue changed includes full ordinary
Shape and One Guest routes, win/Enter restart, diagnostic shared-feature captures,
five animated skin/ragdoll checks, all seven existing menu events (maximum **14**
characters under the **15** cap), finite nonzero native game-output audio and the
movement checks/replays. Diagnostic pose/activation setups are labelled; they are
not represented as ordinary full gameplay. Audio is 48 kHz stereo float game output,
not a microphone recording. These captured controls carry no new FPS/speedup claim.

The extra old timed **Home favor replay fails its completion check**. The native
process exits 0, every action runs, script diagnostics are empty and resets are
zero. At the named cushion-held checkpoint the player is approximately
**(6.5500, 1.6000, 10.5337)** with **no held item**; the caretaker still asks for the
cushion at the later handoff checkpoint. No completion/save is produced. Root cause
is not established: recalibrate/reproduce the old route before attributing a game
regression. The independent diagnostic carry smoke is separate evidence and does
not turn this failed ordinary replay into a pass.

Pool quest/throw replays, eight-favor reward restart, the planned new lifecycle/
pressure campaign and final uncapped timing windows were **not run** after that
failure/queue change. Historical completed campaigns and uncapped measurements
retain their original dated scope; no missing number is invented. Shape's earlier
dispatch polish cost and variable-clock caveat remain as recorded. Windows DX12
acceptance does not establish Linux, Vulkan, Metal or hardware-tier calibration.
The preserved One Guest archive passes integrity but `verify --require-current`
exits **1**, reporting only **compiler preparation code changed**. It remains a
valid preserved functional control, not a falsely current package.

### Reconciliation and next sole writer

| Issue | Evidence-based disposition for parent updates |
| --- | --- |
| #25 | Functionality complete: prior full edge/recovery matrix plus final ordinary win/restart and editor export/recovery acceptance qualify for closure. |
| #26 | Already closed by parent; final ordinary win/restart passes. |
| #24 | Original 144 exports adopted; ordinary sheet ghost and six rail/fence reviews remain open. |
| #27 | Partial: measured engine/resource work retained; complete capacity/hardware/allocator/VRAM audit remains open. |
| #28 | Partial: menu, AI/scripts, transitions and prior bounded ownership evidence retained; tails, sustained attribution and the failed Home replay remain explicit. |
| #29 | Bake bar and targeted acceptance complete; large preview preparation and residual indivisible upload/UI-thread stalls remain open. |
| #30 | Prior exact solver/storage/invalidation retained; historical Lantern peak is still missing, exhaustive copy/allocation accounting and remaining kernel/cancellation attribution remain open. |
| #31 | Partial: Windows skinning/pose reuse, conservative visibility, granular loading and existing lossless packing retained. New portal/job-system/sparse-format work is not declared complete; codec adoption remains deferred. |
| #17 / #18 / #21 | Platform/editor/distribution gates stay open. Windows development binaries do not prove v1, Linux editor or installer/Linux distribution acceptance. |
| #14 / #15 | Target 0.14, outside this phase. |

The explicit user-approved next order is **A)** incremental editor preview plus
splitting remaining indivisible GPU-install/UI-thread operations, building on the
existing chunked uploads/scheduling; **B)** targeted remaining CPU direct/indirect
bake kernels; **C)** whole-record temporary decode/repack memory overlaps; **D)**
game frame tails/render passes at the actual target resolution/backend; then final
combined verification/report/cleanup. **No Zstd/codec adoption** is approved. These
are later sequential tasks, not concurrent-writer permission.

`local/windows-validation-0.13.0/final/evidence-index.json` indexes raw evidence,
accepted binaries, current source identities, successful/failed/interrupted
controls, media receipts and machine-readable issue dispositions in `C:\Users\Connor Dawkins\Documents\Codex\2026-10-23\task-6\final-evidence`.
All unique attempts, partial staging, recovery material, detached baseline evidence
and earlier media are preserved. **23 executable/DLL/symbol files** are frozen in
`accepted-bin/` outside `target`. The original target is also **retained** for the
new queue: **cargo clean is not run**, and awake helper **PID 34744** remains under
queue custody, SYSTEM-required-only, expiry **2026-10-24 17:57:20 UTC**. No supported
stop file is created and no permanent power/security setting is changed. Final
commit/remote/clean status and writer release are recorded in the ownership handoff.
No later pass, CI wait, main merge, release, force push or version bump occurs here.

## Editor preview and install pass A - Windows, 2026-10-23

Pass A starts from clean, remote-verified
`44b6c07dfd6090447a614360e8a41d303d172de8` on `0.13.0-staging`.
The 1,270-object Lantern Hollow source and the previously preserved Arcade package
were reused. Original fixtures, earlier failures, native receipts and binaries
remain intact. New evidence is outside `target` at
`C:\Users\Connor Dawkins\Documents\Codex\2026-10-23\task-7\pass-a-evidence`;
the validation root's `optimization-pass-a/` index points to it.

Inspect used about 4.75–4.87 seconds lighting prop vertices whose RGB the same
preview immediately overwrote with studio shading. The new path keeps the exact
geometry, normals, UVs, alpha, source albedo, tint and final studio RGB while
skipping those discarded light samples. Environment keeps its authored lighting.
One previous complete build can be shared for unchanged geometry/metadata;
unaffected Inspect prop buffers can be shared across architecture edits. The
optional owner is limited to a quarter of the existing CPU admission budget,
capped at 512 MiB; this is one prior scene, with no history. Mutable lighting
uses copy-on-write buffers. Larger builds reuse only a weak reference to an already live display or queued
result, so an abandoned scene can be collected without a cache owner.

Reuse keys include source geometry, supports, fixture/model identities, material
metadata and resolved texture identities. Model GLB and character appearance PNG
bytes are checked on each request. Failed and cancelled loads retry, and cancelled
model/occluder preparation cannot publish a partial reusable owner. A changed
lighting/material/geometry input invalidates the relevant prepared result.
Picking bounds publish with the installed scene; viewport picking waits during
preparation while the scene list and inspector remain available.

The existing texture stage now runs the identical CPU mip algorithm in one
bounded process-wide batch, then installs one ready sheet per handler. The same
path prewarms character/door sheets. A device-only character compute pipeline is
prepared once off the UI thread and retained for that renderer. The alternate
Static-light bank now yields through world slices, textures/materials, prop
batches and decals, using worker-packed geometry. Caustics and primary decals
also prepare before the final commit. Superseding a request drops its pending
ownership, preserves the displayed scene and uses the existing upload fence;
cancelled mip jobs finish at most their current bounded sheet without a UI join.
The existing coalescing, generation checks, unchanged-frame suppression,
256-vertex slices, soft 4 ms quantum and texture admission were retained.

Matched native editor runs use the same 1440x816 drawable and ordinary editor
Fifo presentation. These are latency measurements, **not FPS comparisons**.
No build, bake or screenshot capture overlaps either timing run. Each cell below
is **preparation wall / worker-thread CPU seconds; request-to-ready seconds;
upload-handler p95 / worst milliseconds**. Readiness includes queue/response wait,
UI scheduling, installation and the upload fence; it is not GPU execution time.

| Native operation | Baseline | Pass A |
| --- | --- | --- |
| Initial Inspect | 5.995 / 5.953; 8.259; 29.96 / 103.02 | 1.554 / 1.531; 4.569; 7.38 / 25.53 |
| Room move in Inspect | 5.796 / 5.781; 7.773; 14.77 / 123.12 | 1.374 / 1.375; 3.432; 14.87 / 56.38 |
| Environment after edit | 14.495 / 14.469; 16.410; 6.51 / 156.66 | 14.266 / 14.250; 16.222; 7.29 / 86.62 |
| Return to Inspect | 5.826 / 5.812; 7.724; 6.91 / 125.49 | 1.379 / 1.375; 3.373; 13.61 / 57.72 |
| Undo room move | 5.811 / 5.812; 7.694; 6.26 / 131.15 | 1.376 / 1.375; 3.324; 7.74 / 64.23 |

The developer-only CPU oracle compares complete legacy mesh and prop binary
records: large: 12 exact cases, small: 12 exact cases.
It covers no-op/metadata, wall, undo, Environment, light, material, return-to-Inspect
and prop edits. The reference column below times the preserved legacy builder
alone; the candidate includes input preparation, bounds/characters and packing,
so the two CPU scopes are deliberately stated separately. Short Windows CPU
samples are subject to accounting quantization. These are explicit
campaigns, never full-map bakes or automatic Rust tests.

| Fixture / operation | Legacy build wall s | Pass A complete wall / thread CPU s | Full / prop reuse |
| --- | --- | --- | --- |
| large / cold | 5.365 | 1.604 / 1.547 | False / False |
| large / noop | 5.344 | 0.117 / 0.125 | True / True |
| large / wall | 5.376 | 1.000 / 1.000 | False / True |
| large / metadata | 5.327 | 0.117 / 0.109 | True / True |
| large / noop-environment | 14.037 | 0.125 / 0.125 | True / True |
| large / light | 14.063 | 14.225 / 14.219 | False / False |
| large / material | 14.183 | 14.512 / 14.469 | False / False |
| small / cold | 0.050 | 0.190 / 0.172 | False / False |
| small / noop | 0.042 | 0.019 / 0.016 | True / True |
| small / wall | 0.044 | 0.025 / 0.016 | False / True |
| small / metadata | 0.044 | 0.019 / 0.031 | True / True |
| small / noop-environment | 0.041 | 0.019 / 0.000 | True / True |
| small / light | 0.042 | 0.060 / 0.062 | False / False |
| small / material | 0.044 | 0.080 / 0.062 | False / False |

Arcade's frozen baseline reproduced a 501.05 ms upload handler. The first candidate
reduced p95 but left a 426.61 ms commit: alternate Static bank 212 ms, character
upload/pipeline 75 ms and dynamic doors 135 ms. The final result follows. Both
game runs confirm NVIDIA RTX 3070 Laptop / DX12, physical 1280x720, High textures,
Static lighting, VSync OFF, Immediate presentation and no frame limiter. The
existing Static selector maps to `lighting=low`, `lightmaps=off` and reflections
off in internal diagnostics; texture quality/filtering remain High in both. They
measure loading latency, with no screenshot/build/bake contention, and make no
throughput claim. Active-handler p95 excludes calls at or below 0.1 ms so added
polls cannot alone explain the improvement. Event-pump gaps also include drawing,
driver work and the final game commit, beyond the measured upload handler.

| Arcade | Handler p95 / worst ms (count) | Active-handler p95 ms | Post-CPU-ready pump p95 / worst ms | Request to ready s |
| --- | --- | --- | --- | --- |
| baseline | 49.07 / 501.05 (88) | 51.64 | 49.83 / 897.72 | 3.840 |
| final | 5.50 / 54.88 (629) | 18.66 | 6.56 / 356.19 | 4.270 |

The repeated control measured 496.41 ms
before and 54.20 ms after, with p95
47.07 / 6.18 ms.
These gains concern responsiveness: first-run total readiness increased from
3.840 to 4.270 seconds.
The editor's warmed handler p95 also did not consistently improve. Peak memory
is roughly flat; no general loading-throughput or memory-reduction claim follows.

Cold renderer/device/pipeline initialization before staged installation remains
visible separately in `measured-outcomes.json`; the all-loading pump maximum is
2146.8 ms before and
2392.0 ms after. Shader
worker elapsed time and each final-commit family are logged. Worker CPU, queue
wait, UI response wait, upload-handler time and total readiness are separate
receipts. No GPU timestamp-duration claim is made. The 4 ms budget remains soft:
individual device/upload calls and remaining resource families can exceed it.
The residual longest GPU-stage handlers are whole prop batches (about 54 ms),
with primary/alternate decals about 28 ms. Final commit families measure Static
bank 0.004 ms, characters 6.739 ms and dynamics 7.669 ms. Outside that renderer
commit, game physics-item activation remains 197.728 ms on the UI thread.
Instrumented pump gaps also include expensive developer world snapshots and the
first presented frame; they are not pure GPU or ordinary uninstrumented UI time.
These remaining blocks are explicit limits of the pass.

Windows process lifetime peak working set/private commit was sampled at 20 Hz.
The figures include renderer/driver allocations and are neither a live heap nor
VRAM totals. Process CPU is for the entire matched run, including background
preparation and rendering; it is separate from the submitting worker's CPU time.

| Run | Before peak RSS / private GiB | After peak RSS / private GiB | Whole-process CPU s before / after |
| --- | --- | --- | --- |
| Editor | 1.877 / 3.637 | 1.867 / 3.624 | 51.36 / 31.56 |
| Arcade | 0.818 / 1.230 | 0.816 / 1.229 | 7.56 / 8.83 |

Native controls pass twelve rapid prop edits, selection/inspector use, eight
material-theme changes/search, exact undo/redo restoration, rapid preview-mode
changes and switching levels. Source files remain byte-identical in that run.
Another native run starts large Environment preparation, recovers a different
project through the normal import/open path, and confirms the old unsaved document
was saved exactly. Export/reimport preserves all three declared Shape of an Exit
source files byte-for-byte; package integrity and require-current verification
pass. With no package in the recovered project, ordinary Build+Play bakes, launches
the native game, captures its scene and reaches terminal success; cached Build+Play
also succeeds. The prior handoff's uncertain cold attempt remains preserved;
its original root cause has not been established by this successful reproduction.

The staged Static bank is exercised natively with its four prepared fixtures
off and on again plus a door open/close. Captures show a visible off/on change;
baseline/final comparison receipts are in `static-native-comparison.json`
(3/3 captures exactly identical;
per-channel differences retained for each). These capture runs are excluded from
timings. The real bake-progress implementation is unchanged and is visible during
the new ordinary export/Build+Play validation.

Final formatting, strict workspace/all-targets/all-features Clippy, workspace
Rust tests (**301 passed, zero failed/ignored**) and locked release
workspace/all-features build pass. Asset checks pass: 629 assets, 389 props,
140 textures with the 100 existing texture advisories, and the 21 relevant Python
tests. The broad Python suite's 17 reproduced baseline failures/errors remain an
explicit limitation. Tiny new Rust regressions protect neutral output, precise
reuse/cancellation and cold occluder retry without shipped assets, GPU or map bakes.

Native Library media (sampled screens with truthful two-second holds; timing
overlays come from separate capture-free runs):

- [Places-pass-A-before-after.mp4](https://chatgpt.com/api/library/files/libfile_ee06614a82588191ae6f4e1e47a640bb/download) — `libfile_ee06614a82588191ae6f4e1e47a640bb`.
- [Places-pass-A-native-editor.png](https://chatgpt.com/api/library/files/libfile_1b19d61237e48191af1e57daca1b0f98/download) — `libfile_1b19d61237e48191af1e57daca1b0f98`.
- [Places-pass-A-real-bake-progress.png](https://chatgpt.com/api/library/files/libfile_aff638f8e6288191a737f2baa4944e5f/download) — `libfile_aff638f8e6288191a737f2baa4944e5f`.
- [Places-pass-A-cold-recovered-play.png](https://chatgpt.com/api/library/files/libfile_78392b25f2b88191b177237b9cb94b34/download) — `libfile_78392b25f2b88191b177237b9cb94b34`.

Library confirms all four saved files. Its local metadata helper cannot apply
extended attributes on this Windows Python runtime (`os.setxattr` unavailable);
the returned IDs, original hashes and exact failed metadata receipts remain in
`media/`. This does not affect the confirmed Library saves; no upload was retried.

**Final combined acceptance remains incomplete.** Home's previously failed timed
cushion pickup/replay remains for final gameplay validation; its cause remains
unresolved, and this pass does not edit pickup/favor logic. Pool favor/throw replay, reward restart, the new resource
pressure campaign and final uncapped windows remain pending. Real Environment
recomputation still performs its lighting work. The authoritative remaining sequence is: (2) upgrade all Rust workspace
dependencies to newest stable and assess the largest project-owned Rust files;
validate migrations before new optimization baselines; (3) one lighting/compiler
pass, first CPU direct/indirect kernels, then calculation/storage redundancy and
worthwhile bounded compression/format/packing for exported map size, with separate
attributable substeps; (4) major steady runtime RAM reduction and transient
load/decode/repack overlap, using Lantern Hollow and the new representation;
(5) game frame tails/render passes on the final dependency/data foundation;
(6) final integrated validation and issue reconciliation, this one canonical
report, preserved deliverables, then cargo clean and awake-helper shutdown only
after ALL work completes. Parent coordinates sequential handoffs; no concurrent
tasks or later passes were started in this checkout.

The dependency task must inspect every manifest/lockfile dependency class and
transitive constraints against actual stable registries, migrate major versions,
identify genuine blockers and validate wgpu/SDL/Rapier/Luau/platform integration.
Cargo update alone is insufficient; prereleases are not the default. The
largest-file task is an assessment of responsibilities, duplication, coupling,
hotspots and compile/maintenance cost, with justified extraction recommendations,
risks/tests and cohesive large files best left intact. It authorizes no blanket
rewrite or arbitrary line-count target.

The user's later update supersedes the earlier Zstd exclusion; no codec change
is part of A. The lighting/storage step compares single/multi-level exports,
compressed/uncompressed record and lighting components, per-light/circuit/state
redundancy, duplicate/empty/constant regions, formats, precision, packing and shared
data. Prior Full Arcade occupancy is 93.60%; prior Zstd9 savings are 6.65%, with
decoder-window limits unresolved. Default to lossless output-equivalent wins,
explicit hostile-input bounds and backward compatibility; measure full-bake time,
package bytes, load/decode time and peak memory. Quality-changing proposals must
be explicit and separate.

The RAM task must reproduce the user's near-2 GB Lantern Hollow report without
treating it as a verified breakdown. Separate process private/commit and working
set from GPU dedicated/shared memory; distinguish startup, peak, settled and
unloaded ownership. Attribute lightmaps, textures, duplicate meshes, collision/nav,
lighting transport, caches/scripts and allocator/driver retention; prefer general
engine fixes. Protect quality, physics, light switches, water, FPS, load time and
repeated-transition cleanup. Smaller package files and lower resident RAM are
distinct outcomes. Carry unresolved Home replay and recheck recovered Play after
the later representation/dependency changes. No CI wait, main merge, release or version bump occurred.

Temporary SYSTEM-required-only helper PID 34744 remains under parent/next-writer
custody until the queue completes, expiring `2026-10-24T17:57:20.8514205Z`. Its
original `2026-10-22/task/keep_awake.ps1` and `keep-awake.stop` mechanism remain
unchanged; the stop file is absent. No permanent power/security setting changed.


## Dependency foundation, phase 2 — Windows, 2026-10-23

This phase starts from clean, remote-verified `410da225603054c1f0a3c42bcd0154af1c6a45e5`
on `0.13.0-staging`. The sole workspace member has one `Cargo.toml` and one
`Cargo.lock`: **18 unique direct dependencies, 22 declarations**, including the
build-time SHA-256 dependency, Unix libc, non-WASM audio/executor dependencies and
the Linux Vulkan/macOS Metal/Windows DX12 feature selections. There are no direct
dev dependencies, git pins, alternate registries, patches or vendored Rust forks.
All **198 registry crate names / 206 locked registry packages** were checked against
the live crates.io sparse index, selecting non-yanked stable releases, with URL,
retrieval time, index hash, exact release records and parent requirements retained.
This is an audit beyond the manifest's existing semver ranges, not merely a
`cargo update` claim. At this checkpoint the core major versions were already
the newest published stable releases; no newer-major API migration was necessary.

### Direct requirements and compatible lockfile updates

The manifest now raises ZIP and Unix libc to the verified newest stable releases.
All direct declarations resolve to their newest stable version. Features remain
unchanged, including Luau vendoring/send without JIT, serde JSON float round-trip,
ZIP's existing deflate backend, WAV/PCM audio and the target-specific graphics backends.

| Direct crate | Previous declaration | Verified final stable |
| --- | --- | --- |
| bytemuck | 1.25.2 | 1.25.2 |
| ctrlc | 3.5.2 | 3.5.2 |
| glam | 0.34.1 | 0.34.1 |
| kira | 0.12.5 | 0.12.5 |
| libc | 0.2.190 | 0.2.191 |
| mlua | 0.12.2 | 0.12.2 |
| png | 0.18.1 | 0.18.1 |
| pollster | 1.0.1 | 1.0.1 |
| rapier3d | 0.36.1 | 0.36.1 |
| rayon | 1.12.0 | 1.12.0 |
| rtrb | 0.4.0 | 0.4.0 |
| sdl3 | 0.20.0 | 0.20.0 |
| serde | 1.0.229 | 1.0.229 |
| serde_json | 1.0.154 | 1.0.154 |
| sha2 | 0.11.1 | 0.11.1 |
| terminal_size | 0.4.4 | 0.4.4 |
| wgpu | 30.0.1 | 30.0.1 |
| zip | 9.0.3 | 9.0.5 |

Eight lockfile packages change; every other selected version is unchanged:

| Package | Before | After |
| --- | --- | --- |
| cc | 1.8.0 | 1.10.0 |
| crc32fast | 1.5.2 | 1.5.3 |
| find-msvc-tools | 0.1.14 | 0.1.15 |
| libc | 0.2.190 | 0.2.191 |
| smallvec | 1.16.3 | 1.16.4 |
| tokio | 1.53.3 | 1.54.1 |
| typed-path | 0.12.3 | 0.12.4 |
| zip | 9.0.3 | 9.0.5 |

The repository Rust/Cargo **1.99.1** pin and `rust-version = "1.99"` remain
unchanged; all selected declared Rust requirements fit that policy. ZIP 9.0.4's
verbatim-component separator rejection and 9.0.5's platform/raw-copy fixes are
included. No decoder bound, package limit, feature or useful dependency was removed.
Checksums remain in the lockfile; no prerelease or yanked release is selected.

### Upstream constraints, duplicates and native sources

The following retained versions have a newer stable release outside their
parents' published requirements. Parent chains are recorded in
`dependency-summary.json` and the individual registry records, including each
platform predicate and dependency kind. The table lists the actual incoming
requirements; a parent can itself be held by a higher parent. These are upstream
constraints, not project pins that were left unexamined. No major-version override
or local fork is used to misrepresent them as compatible upgrades.

| Crate | Retained older version(s) | Newest stable | Incoming upstream constraints |
| --- | --- | --- | --- |
| allocator-api2 | 0.2.21 | 0.4.0 | hashbrown ^0.2.9 |
| bit-set | 0.10.0 | 0.11.1 | naga ^0.10; wgpu-core ^0.10; wgpu-hal ^0.10 |
| bit-vec | 0.9.1 | 0.10.1 | bit-set ^0.9.1; wgpu-core ^0.9.1 |
| extended | 0.1.0 | 0.2.0 | symphonia-format-riff ^0.1.0 |
| glam | 0.30.10, 0.31.1, 0.32.1, 0.33.12 | 0.34.1 | glamx ^0.33.7; kira ^0.33.0; nalgebra ^0.30; nalgebra ^0.31; nalgebra ^0.32; nalgebra ^0.33 |
| hash32 | 0.3.1 | 1.0.0 | heapless ^0.3.0 |
| hashbrown | 0.16.1 | 0.17.1 | gpu-allocator ^0.16.0; spade ^0.16.1 |
| heapless | 0.8.0 | 0.9.3 | rstar ^0.8 |
| libloading | 0.8.9 | 0.9.0 | ash ^0.8; wgpu-hal ^0.8 |
| lock_api | 0.4.14 | 0.5.0 | parking_lot ^0.4.14 |
| miniz_oxide | 0.8.9 | 0.9.1 | png ^0.8 |
| num-bigint | 0.4.8 | 0.5.1 | num-rational ^0.4.0 |
| parking_lot | 0.12.5 | 0.13.0 | mlua ^0.12; wgpu-core ^0.12.3; wgpu-hal ^0.12.3 |
| parking_lot_core | 0.9.12 | 0.10.0 | parking_lot ^0.9.12 |
| r-efi | 6.0.0 | 7.1.0 | getrandom ^6 |
| redox_syscall | 0.5.18 | 0.9.5 |  |
| rustc-hash | 1.1.0 | 2.1.3 | naga ^1.1; naga-types ^1.1; wgpu-core ^1.1 |
| syn | 2.0.119 | 3.0.9 | nalgebra-macros ^2.0; profiling-procmacros ^2.0; windows-implement ^2.0; windows-interface ^2.0; zerocopy-derive ^2.0.46 |
| typed-path | 0.12.4 | 0.13.0 | zip ^0.12 |
| windows-collections | 0.3.2 | 0.100.0 | windows ^0.3.2 |
| windows-core | 0.62.2 | 0.100.0 | wgpu-hal ^0.62; windows ^0.62.2; windows-collections ^0.62.2; windows-future ^0.62.2; windows-numerics ^0.62.2 |
| windows-future | 0.3.2 | 0.100.0 | windows ^0.3.2 |
| windows-implement | 0.60.2 | 0.100.0 | windows-core ^0.60.2 |
| windows-interface | 0.59.3 | 0.100.0 | windows-core ^0.59.3 |
| windows-link | 0.2.1 | 0.100.0 | libloading ^0.2; parking_lot_core ^0.2.0; windows-core ^0.2.1; windows-future ^0.2.1; windows-numerics ^0.2.1; windows-result ^0.2.1; windows-strings ^0.2.1; windows-sys ^0.2.1; windows-threading ^0.2.1 |
| windows-numerics | 0.3.1 | 0.100.0 | windows ^0.3.1 |
| windows-result | 0.4.1 | 0.100.0 | windows-core ^0.4.1 |
| windows-strings | 0.5.1 | 0.100.0 | windows-core ^0.5.1 |
| windows-threading | 0.2.1 | 0.100.0 | windows-future ^0.2.1 |

The lockfile also preserves optional cross-version nalgebra conversions and
platform dependencies. Windows `cargo tree --duplicates` does not show every
all-target lock entry; both views are retained. The apparent repeated SDL sys
entry is its build/normal use, not two different SDL versions. In particular,
Rapier/glamx/Kira's glam 0.33 cannot be forced to the application's glam 0.34,
and wgpu's Windows 0.62 family cannot be replaced with 0.100 behind its back.

[SDL's latest stable release](https://github.com/libsdl-org/SDL/releases/tag/release-3.4.18)
is **3.4.18**, matching sdl3-sys `0.7.2+SDL-3.4.18`, installed x64 headers/libs
and the retained runtime DLL (SHA-256 `2b4b128d8d510cc2d442693cf848c050d700d43dc667afe9e5fd82b0570d667e`).
The latest published `luau0-src` is **0.22.0+luau740**, required by latest
mlua-sys 0.13 / mlua 0.12.2. Upstream [Luau 0.743](https://github.com/luau-lang/luau/releases/tag/0.743)
is newer than that published Rust source package; updating beyond its supported
published binding/source pair would require independent vendoring and is not
claimed done. DXC/DXIL 1.8.2502.11 are retained and hashed. Linux/macOS dependency
resolution is audited; no Linux/Vulkan/Metal hardware execution is claimed.

Formatting and strict workspace/all-target/all-feature Clippy pass on the
dependency-only checkpoint. Its Rust test compilation stops after 502.9 seconds
with **no space on device**, before tests run. The failed attempt is retained;
reversible NTFS compression of generated debug artifacts recovers space without
deleting accepted binaries, unique evidence or running `cargo clean`. The
following focused structure commit and the final combined Rust/release/native
gates are recorded below; this checkpoint alone makes no test-suite or final
integrated acceptance claim.

Raw audit, before/after metadata and lockfile, registry evidence and logs are in
`C:\Users\Connor Dawkins\Documents\Codex\2026-10-23\task-8\dependency-evidence`,
outside `target`. The canonical report remains this file; no separate release
report is introduced.


### Largest-file reconciliation and authorized focused refactors

The earlier read-only assessment at `44b6c07` covered 447 project-owned Rust
files. It is retained rather than repeated. Pass A added seven Rust files before
this phase. Rechecking the affected boundaries confirms the recommendations;
this phase adds nine files net, for **463 project-owned Rust files** after the
extractions and the one new regression test. File counts are context, not targets.

The user's later instruction authorizes automatically queuing appropriate
behavior-preserving refactors. The following two are implemented separately from
the dependency commit:

| Boundary | Before | Final responsibility |
| --- | --- | --- |
| `engine/loader.rs` | 6,565 lines: catalogs, fixture loading, source admission and many domain checks | 426 lines for the public compatibility paths, catalog and fixture loading; `loader/validation/mod.rs` retains the original ordered `validate_level` body. Six private domain modules own environment/budgets, geometry, lighting, props/rigid authoring, motion/doors and entities. |
| `engine/level.rs` | 11,787 lines: authoring definitions, borrowed geometry and owned runtime collision/codecs | 9,645 lines for authoring and borrowed surfaces; `level/runtime_collision.rs` owns the unchanged 2,141-line runtime models/codecs plus explicit imports and module documentation (2,156 lines total). Public floor, ceiling, water and ladder paths are reexported. |
| `level/physics.rs` | Private access to owned floor/ceiling details | Moved unchanged under `runtime_collision/physics.rs`; fields stay private rather than widening them for a sibling module. |

The ordered facade preserves every call and early return, messages and first-error
ordering. `LevelIndex` and its capability facts stay inside the private validation
family; only the sequence-owner handoff is visible to its facade. Historical
`loader::validate_level`, lifetime constant, rigid-prop validator, script-call
enumerator and compiler wall-query paths remain available at their existing
visibility. Compilation caught the compiler's wall-query import and private-module
visibility requirements; these were fixed through reexports, without suppressions.

The source oracle checks **99 validator/type bodies**, the ordered facade, every
runtime collision token and the private physics implementation. Function bodies,
arithmetic, codec field order, bounds and diagnostics are unchanged. One tiny
metadata-only test uniquely protects the cross-domain first-error contract with
simultaneous schema/transition/room/prop failures and exact error text. It uses no
assets, renderer, physics simulation or map bake. Its first setup mistakenly fed
an invalid schema to the earlier JSON decoder; that retained failed attempt is
corrected by mutating a successfully decoded fixture in memory. The final suite
has **302 passes, zero failures/ignored**, with about one second of test execution;
the longer gate time is compilation. No full-map test or automatic bake is added.

**Queued for phase 3:** extract the approximately 1,200-line repair planner from
`map_compiler/geometry_check.rs` as a private submodule alongside the next compiler
work. The file remains **4,931 lines**, unchanged here. Preserve the shared issue
classification, mandatory post-repair safety check, JSON diagnostics and exit
contracts together; validate a narrow repair/refusal fixture matrix. Moving it
now would add compiler coordination before the related work, without avoiding a
demonstrated duplicate edit. Its disposition is an explicit queue item, not a
forgotten recommendation or an implemented claim.

The prior renderer/app/entity-orchestration recommendations remain limited:
do not broadly rewrite those large stateful owners; transport and glTF remain
cohesive. The entity validation graph remains one domain because the shared index,
capabilities, actions, bindings and sequence/cycle checks are coupled. This change
improves ownership and review boundaries; it establishes **no FPS or compile-time
speedup**. Later measured compiler/storage, memory and game-frame passes remain
separate.


### Final phase-2 gates and native acceptance

The final frozen source passes formatting, strict
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace` (**302 passed, zero failed/ignored**) and
`cargo build --locked --release --workspace --all-features` (**exit 0**, 1,026.3 s).
All **478 source/build-input hashes** and **21 frozen executable/DLL/symbol files**
are rechecked. The nine current workspace executables, their symbols and required
SDL/DXC/DXIL payloads are preserved outside `target`; obsolete retained tools are
not represented as newly built workspace outputs. Assets pass **629 / zero warnings**,
props **389**, textures **140 / the 100 existing advisories**, and all **21** relevant
Python tests. The broad suite remains **366 tests, 8 failures, 9 errors, 16 skips**;
its exact 17 failure/error headers match the preserved `d5a0628` Windows baseline,
with no new header. It is not described as green.

All **13 sequential acceptance stages pass** after the release command exits;
no build/bake/capture overlaps an FPS timing window. Native coverage includes:

- **Movement and formats:** all 17 ordinary controller command trajectories are
  byte-exact against the preserved pre-upgrade executable, covering **4,080 ticks**.
  Independent forced small `test_room` and `water_transmission` Static/Full bakes
  retain all **29 non-provenance archive entries** exactly, including semantics,
  geometry, lighting, collision, navigation, lightmaps and probe records as present.
  Each original compiler verifies its own package current; the final reader validates
  both generations. Only compiler-code/tool provenance and the verified corresponding
  manifest fingerprints and provenance-entry digest differ. No stored fingerprint
  or record was edited to obtain equality. The prior expensive full-map solver
  evidence remains preserved; no shipped-map Cargo/CI bake was introduced.
- **Editor/compiler:** native Lantern preview, room edit, Inspect/Environment changes
  and undo preserve the source and restore the document exactly. The current capture
  run observes roughly **1.36 s** room-edit preparation, **1.35 s** return-to-Inspect
  and **14.52 s** Environment preparation, consistent with the retained distinction
  between reusable Inspect work and real Environment recomputation; these capture
  controls are not matched new latency benchmarks. Native Shape export/reimport
  preserves manifest, level and Luau source bytes exactly, passes integrity/currentness,
  saves the old unsaved document while superseding its preview, then completes both
  genuinely cold recovered Build+Play and cached Play. The native child is captured.
  The actual named bake-progress display is visually reviewed, including its honest
  indeterminate state. The earlier uncertain attempt remains preserved separately.
- **Games and shared systems:** both Shape of an Exit and One Guest Too Many complete
  their ordinary win/Enter-restart routes. Diagnostic shared controls cover carry,
  Pool water/doors/reflections, lab quality/reload, blood, five animated skins with
  ragdolls and reset/original-black restoration, and native stair/ramp/crouch movement.
  Game-output audio is finite and nonzero, **48 kHz stereo float**, with **880,320**
  captured frames; it is not microphone audio. These diagnostics do not substitute
  for the outstanding ordinary Home/Pool favor and reward campaigns.
- **Prepared lighting and staged loading:** all three on/off/restored captures are
  pixel-identical to the frozen Pass A controls; off visibly differs from on.
  Capture-free Arcade loading records a **54.82 ms** worst upload handler and
  **5.69 ms p95**, preserving the prior responsiveness range. The soft budget,
  remaining whole resource calls and roughly 198 ms historical physics-item
  activation limit remain; this phase claims no new latency or FPS speedup.

The final current-only sanity windows use NVIDIA RTX 3070 Laptop **DX12**, physical
**1280x720**, High texture/filtering, VSync **off**, actual post-settings presentation
**Immediate**, and no frame limiter. Startup device logs initially say Fifo; the
later actual mode change and physical window line are retained. The active hybrid
display is **2560x1440 at 165 Hz**. Observed ready presentations and loop rates
exceed that refresh rate, with no ready misses or refresh-rate plateau. Thus these
are uncapped engine-throughput sanity observations, not a before/after improvement
claim or the later dedicated optimization baseline. Capture-heavy controls and
editor Fifo views are excluded. Functional cold-child capture uses its own normal
play window; it is not represented as a 720p FPS run.

| Case | FPS | Mean / p95 loop ms | Timed samples | Ready presentations / misses |
| --- | --- | --- | --- | --- |
| home | 550.2 | 1.818 / 3.132 | 2712 | 5424 / 0 |
| pool | 472.4 | 2.117 / 4.003 | 2346 | 4691 / 0 |
| lab | 294.4 | 3.396 / 5.527 | 1446 | 2891 / 0 |

This is **Windows acceptance only**. Registry/platform resolution and static Unix
libc API inspection do not establish Linux/Vulkan or macOS/Metal execution.

The crates.io index was rechecked for all **198 names at 23:12 UTC**: no newer
stable release appeared during validation. All 18 direct crates are current;
**32 older versions across 29 transitive names** remain only under the exact
upstream constraints listed above. The newest published Rust Luau source package
still carries 0.740, with the separately documented upstream 0.743 packaging gap.

The disk-full attempt and the first new-test setup failure remain in the evidence.
During validation, reversible NTFS compression of generated debug outputs and
retained CSV/JSONL/log evidence recovered headroom without deleting unique data;
sampled before/after hashes match. Existing package/capture/source bytes and the
release executables were unchanged. Log-file filters left evidence directories
uncompressed. This was not exported-map compression or a package-size optimization.

**The user's confirmed between-run cleanup instruction supersedes the earlier
blanket deferral until the entire queue ends.** At this completed phase boundary,
all build and native processes had exited. The **21 current executable/DLL/symbol
files** and **two legacy probe files** were preserved and hash-verified outside
`target`, with media, recovery projects, comparison packages and source evidence.
Only verified disposable task-owned staging links were removed: **17 hardlinks**,
each checked against its still-existing original package and recorded SHA-256.
An additional 94 conservatively copied Cargo build-script/proc-macro files were
classified from package metadata as reproducible cache and removed as well.
No unrelated system-temp sweep occurred; no newly created `rustc*`/`places-*`
system-temp candidates were found in the ownership check.

`cargo clean --target-dir C:\Users\Connor Dawkins\Documents\Places\target`
then exited **0** and removed `target`. The observed volume-free-space increase
from Cargo cleanup was **17.44 GiB actual**
(**18,725,871,616 bytes**), distinct from Cargo's
**30.8 GiB logical / 22,133 files** report. The disposable cache-copy cleanup
recovered another **113,758,208 actual bytes**. Free space afterward is about
**25.51 GiB**. Frozen `places-compile --version` still runs successfully
after cleanup, and preserved binaries are hash-checked again. The next Rust phase
must rebuild Cargo output; validated baseline executables are in
`task-8/dependency-evidence/final-bin/`, not in the removed `target/release`.

The scoped pre-clean inventory identifies these largest retained queue consumers:
October 23 **Task 2: 17.25 GiB**, **Task 4: 9.38 GiB**, **Task 5: 7.69 GiB**;
Places GPU-rendering evidence **7.28 GiB**, storage/compiler evidence **6.89 GiB**
and loading/memory evidence **4.43 GiB**. `target/debug` reported about **12.18 GiB**
of deduplicated compressed-file storage in that snapshot, **28.93 GiB logical**.
These per-folder storage estimates are not additive across hardlinked folders;
actual reclamation above is measured using volume free space. Task 5 includes two
**3.05 GiB raw RGBA videos**, retained as prior unique media/comparison evidence.
The audit covers Places and the October 22/23 queue workspaces, not unrelated disk
contents. Earlier evidence ownership is respected rather than assuming every large
file is disposable. Detailed paths, file identities and allocation estimates are
in `disk-before-clean.json`; the cleanup receipts list every removed staging link.

**Phase 2 is complete; the whole queue is not.** Home's old ordinary timed cushion
pickup/favor replay still has an unresolved failure. Pool favor/throw, reward unlock
and restart, resource pressure and final integrated performance remain pending.
Next is phase 3: CPU lighting/compiler kernels, then calculation/storage/redundancy
and bounded worthwhile compression/packing review, with the queued repair-planner
extraction; then Lantern steady/transient RAM, game frame tails, and final combined
verification/issue reconciliation. No later optimization phase began here.

The standing rule for **every subsequent completed Rust phase/run** is: preserve
needed outputs outside `target`, verify preservation and no active target users,
then run `cargo clean` and remove only proven task-owned disposable temporaries.
Keep source, unique data, recovery/comparison evidence and artifacts needed by
unfinished work. Do not sweep unrelated system temp or clean during active builds.

The sole-writer handoff and evidence index are saved under
`local/windows-validation-0.13.0/dependency-phase2/` and the canonical
`final/ownership-handoff.json`. Final commit/remote/clean verification is recorded
there. The existing temporary SYSTEM-required-only awake helper **PID 34744** stays
active under next-writer custody, with its original stop mechanism and expiry
**2026-10-24 17:57:20 UTC**. Stopping it still waits until the whole queue finishes.
No stop file, permanent power/security change, CI wait, main merge, release, force
push, version bump or executable Library upload occurs. All unique evidence remains.


## Phase 3 — lighting/compiler, exact storage and bounded compression

**This phase is complete; the whole queue is not.** The separate implementation
commits on `0.13.0-staging` are:

- Planner extraction: `dd8a42b722ac3a20ec42551ef519b8d7429261e6`.
- CPU light transport: `4e37872932540a8e83cc57f4fbbdbed0ffefbc6c`.
- Exact storage, bounded codec and coordinated tooling: `58c81b34f2644fdbb345e27a78222dc87fc87e32`.

No application version bump, main merge, release, force push or GitHub CI wait
occurred. The compiler still reports **0.13.0**. The final report commit, live
remote verification and sole-writer release are recorded in the canonical
`local/windows-validation-0.13.0/final/ownership-handoff.json` after publication.

### CPU lighting and compiler measurements

Frozen phase-2 executables provide the control. The retained changes cache
immutable local-emitter quadrature/world positions, extents, range and strength;
cache global source directions/weights; reuse squared point distances; and hoist
receiver hemisphere axes out of the gather-ray loop. Sample/evaluation order,
visibility predicates, density, samples, bounces and precision remain unchanged.
The direct stage supplies the measured gain. Indirect-stage changes are small
and are not claimed as a meaningful independent speedup.

Two clean forced Full bakes per side ran sequentially, with the second pass in
reverse candidate/control order, 12 workers, no concurrent build/bake/game and
no disk-cleanup contention. OS file caches were not flushed. Results are medians;
individual runs, CPU times, worker/job counts, stage times and peaks remain in
`paired-bake-profiles.json` and `paired-bake-summary.json`.

| Cohort | Control wall | Candidate wall | Wall reduction | CPU reduction |
| --- | ---: | ---: | ---: | ---: |
| Home Full | 90.993 s | 88.637 s | 2.59% | 2.56% |
| Pool Full | 233.630 s | 222.905 s | 4.59% | 4.79% |
| Beach Full | 286.573 s | 278.558 s | 2.80% | 2.85% |

Run variation is retained, particularly Pool's 219.959/225.851 s candidate and
227.197/240.062 s control samples; this is a two-run host-specific comparison,
not a statistical or universal speedup claim. Direct-stage medians are
Home **38.163→35.988 s**, Pool **117.683→108.894 s**, Beach **69.685→63.921 s**.
Scene and logical/geometric work counts are exact: Home 781,491 receivers and
575,325,441 direct rays; Pool 1,262,575 and 1,364,243,940; Beach 6,907,832 and
1,028,645,568. Bounce counts are unchanged too. Whole-bake peak working sets
remain approximately **0.575 / 0.883 / 2.398 GB**, with no whole-bake RAM win claimed.
Lantern's separate **Static/off** cache run is **38.162→36.903 s**, exact decoded;
it is not a substituted Full cohort or a settled runtime-memory measurement.

Earlier Home **100.337→89.568 s**, Pool **344.893→228.453 s**, Beach
**342.038→288.303 s** wins belong to the prior parallel-filter/grouped-geometry
pass, not this pass. Existing deterministic contiguous jobs, reusable worker
scratch and bounded base/first-circuit sharing remain. Earlier 4/8/12-worker and
256/1024-job experiments are reused rather than rebaking them gratuitously.
No persistent receiver×ray×bounce transfer matrix was introduced. The rejected
GPU prototype's **151/16,384** raw Pool predicate mismatches/setup cost remain
disqualifying; CPU predicates were not relaxed or blindly reimplemented. BVH
builds at **12.7–17.9 ms** remain outside the primary bottleneck.

The parent supplied a **post-cohort read-only** power audit: Best Performance
overlay, Passive AC cooling, Ryzen Controller running with a saved 5 W STAPM /
75°C preset whose enforcement is unknown, and an idle NVIDIA ceiling snapshot
34 W versus 80 W default/100 W maximum. There was no idle slowdown flag and
charger wattage is unknown. No host power/profile setting changed in this phase;
these observations do not establish active throttling throughout the runs.

### Stored data, compression and allocation

PLC1 revision 2 adds exact uniform/empty 16-square tiles: six decoded binary16
coefficients once, including short edge tiles and signed-zero behavior. Existing
nonuniform packed decisions/bits remain. Migration compacts the owned record in
place without solving or requantizing; original capacity is retained, so this
in-place step is not claimed as an immediate RSS shrink. Existing PPS4 prop/state
factoring and content-addressed project sharing are in both controls and are not
credited again. Actual role/header/recovery/source accounting counts shared
physical records once in `final-storage-breakdown.json`.

| Actual package | Deflate-9 control bytes | Balanced bytes | Explicit compact bytes |
| --- | ---: | ---: | ---: |
| Lab, ordinary | 7,211,461 | 7,068,616 | 6,715,347 |
| Nine-level mixed project | 217,543,084 | 203,072,809 | 186,908,950 |

The nine-level reductions are **6.65% balanced / 14.08% compact**. Its uncompressed
ZIP-record bytes fall **742,244,970→667,098,207**; Lab falls
**31,376,942→23,745,381**. These are stored record bytes, not expanded runtime RAM.
The control is normalized through the current phase-2 optimizer and reproduces
the preserved original sizes. Lab's atlas record shrinks **20,940,328→13,308,720**
bytes and Arcade's **253,482,600→207,151,112**. Existing archive compression already
handles repeated zeros well, so raw reductions are larger than physical savings.

Ordinary builds use Zstandard **6**, balanced `optimize-storage` **9**, and the
explicit slower `--compact` profile **19**. Bootstrap JSON remains Deflate at
level ≤9. Actual nine-level migration takes **61.010 / 197.778 s**; both peak
working sets are about **806.8 MB**, dominated by earlier prop validation.
The codec-only sweep records level-9/19 encoding **9.290 / 143.606 s**, encoder
working-set peaks **291 / 364 MB** and sampled private peaks **291 / 504 MB**.
The isolated sweep also compressed bootstrap JSON, so its pilot bytes are not
substituted for the actual publication table above.

Three counterordered bounded record-read series measure median
**1.010→0.716 / 0.855 s**, CPU **1.469→1.141 / 1.266 s**, peak working set
**257.7→217.0 / 221.1 MB** for control/balanced/compact. These read all stored
records through the same bounded current reader; they are not whole-game load
or cold-disk timings. Native loaders were validated separately.

The shared Rust reader enters ZIP method 93 through raw entries and enforces
**8 MiB history**, **16 MiB conservative live reservation**, **64 KiB input
buffering**, expansion ceiling `compressed*65,536+64 KiB`, existing role/aggregate
output limits, exact declared length, CRC and content-addressed hashes. It rejects
oversized windows, output/ratio violations, every tested truncation, skippable or
concatenated frames and suffix bytes. Valid short-reading streams work. The pinned
Zstd producer levels 1–19 fit the history contract; decoder enforcement is
independent. Rust `zstd`/`zstd-sys` are linked; no external codec program is needed
by the game/editor/compiler. New named capabilities and project transport revision
2 make old readers fail closed; current readers accept legacy Stored/Deflate,
project revision 1 and PLC1 revision 1. Authored project format and ordinary map
major version remain unchanged. Migration requires a separate destination and
preserves sources, decoded semantics and original build provenance.

The page encoder borrows packed half planes and converts editable pixels tile by
tile, avoiding a full float materialization and whole-atlas half scratch copy.
Three counterordered Arcade encoder-process runs measure approximately
**6.780→5.273 s**, **22.2% less CPU**, peak working set **1.934→0.701 GB** and peak
commit **2.049→0.839 GB**. Decoded half planes are exact; packed pages stay
unmaterialized. This isolated process includes I/O (the new writer also syncs),
and is distinct from whole-bake peaks or settled runtime atlas memory.

Level 15's 0.45% extra saving at roughly 4.5× encode time is rejected. Exact
byte/bit/delta rearrangements were inverse-hash verified and measured; byte lanes
hurt Arcade Zstd-9 and save only 0.107% at 19, while bit/delta modes are worse.
Whole-tile dictionaries add complexity with negligible archive benefit. Prior
93.6% atlas density is not treated as a packing opportunity. No resolution,
samples, bounces, precision or lighting detail was silently reduced.

### Validation, tooling and custody

The 1,206-line private repair planner extraction preserves shared wall-joint
classification, post-check safety gate, finding order, JSON/CLI and exit behavior.
Its source-body oracle, **11** old/new CLI cases and **14** repair-applier checks
pass. Loader validation/runtime collision extractions from phase 2 were not redone.

Final gates: **306 Rust tests**, strict all-target/all-feature Clippy, locked
all-feature workspace release, required asset checks and **24 targeted Python
checks** pass. The broad **369-test** run retains exactly the known **8 failures /
9 errors / 16 skips**, with no new failure headers. New tests are tiny format,
security, short-read and codec-boundary fixtures; ordinary Cargo tests/CI do not
bake shipped maps. Compiled Rust/Cargo/shader hashes match the frozen producer.

Canonical decoded oracles check every lightmap half, reconstructed PMP/Off stream,
other record, source and migration provenance. Fresh-bake comparisons exclude only
the documented code/tool and manifest compiler/lighting fingerprints. Small
Static/Full serial-versus-12-worker cases, unchanged-member rebuilds, no-op and
unrelated-file reuse, hostile/truncated input, source recovery, existing-directory
refusal and cancellation/restart pass. Home's cancelled forced bake preserves its
cache and restarts by verified reuse in **0.210 s**. Compact archive-stage cancel
exits 130 about **0.13 s** after request without publishing a partial output.

Eight matched native Full-view pairs are **pixel-exact**, covering Home circuit
on/off, wall seams/shadows, Pool service props and water/transmission, Beach shore,
water and cast shadows. The contact-sheet legend corrects inherited camera labels;
full-resolution screenshots remain unmodified. Arcade's eight Full and eight
Static UV/restroom states are pixel-exact; compact Full matches balanced in every
state. Ordinary Lab compact/balanced and matched Lantern Static captures are exact.
Complete nine-level contexts validate transitions; per-level bake caches are not
misrepresented as standalone distributable maps. The native composition contains
four fresh members and five preserved representative members, with original
fingerprints and all 20 recovery files intact; it is not an all-level fresh bake.

Native editor export, current-input validation, cross-project recovery while a
preview is active, exact source recovery and cold/cached Play pass. Ordinary Shape
and One Guest routes/restarts, shared interaction/blood/mirror/cycle checks,
five skins/ragdolls/reset and native stairs/ramp/crouch checks pass. All **17**
movement trajectories / **4,080 ticks** match the preserved control byte-for-byte.
A legacy helper first failed before rendering because Cargo output lacked DXC
DLLs. It was repaired with verified matching links from the complete frozen
payload, then all shared checks passed; the failed receipt is retained.

Presentation sanity: **Direct3D 12**, **Immediate**,
physical drawable **1280×720**, swap interval **0**, VSync off, no frame limiter,
fixed delta or capture during timing, 300 warmup and 1,500 measured frames.
Mean **414.63 FPS**, median **601.07**, above the
reported **165 Hz** desktop refresh with no refresh plateau.
This is not a runtime throughput gain comparison or final integrated acceptance.

Python is developer/release automation, not executable runtime logic. Actual
compressed-payload readers share the bounded Rust compatibility adapter; eight
bootstrap-only tools were restored unchanged. The exact purposes/callers and
temporary scaffolding are in `python-tool-inventory.json`. Set
`PLACES_PACKAGE_REPACK` to preserved `final-bin/package-repack.exe` when using
Python payload tools after Cargo cleanup.

Evidence is under **`task-9/compiler-evidence/`**, indexed by
`local/windows-validation-0.13.0/compiler-phase3/evidence-index.json`. Key receipts:
`candidate18-gates.json`, `paired-bake-*.json`, `final-storage-*.json`,
`transport-review15/packing-process-summary.json`, `final-source-validation.json`,
`native-lighting-comparisons.json`, `human-visual-review.json`,
`Places-phase3-lighting-before-after.png`, native/editor/shared acceptance files,
`native-fps/acceptance.json`, `artifact-preservation.json` and `cargo-clean.json`.

The **21 current executable/DLL/symbol files** and the earlier exact-measurement
producer files are preserved and hash-verified outside `target`, with **18 required
comparison packages**, **206 PNGs**, unique sources and recovery evidence. Identical
archive copies share verified storage; all evidence paths/bytes remain. No raw
RGBA recording or executable Library bundle was created/uploaded. Task-owned
staging aliases and compatibility copies are removed between completed runs.

With no active build/bake/game or target executable, verified workspace-only
`cargo clean` exited 0 and removed target: **8,033 files / 10.6 GiB logical**,
**9,907,314,688 actual bytes (9.23 GiB)** recovered.
Archive consolidation recovered another **210,079,744 actual bytes**. Free space
after Cargo cleanup is **86,548,312,064 bytes (80.60 GiB)**.
This does not credit the parent's separate cross-project/old-work cleanup.
Preserved payload hashes still match and `places-compile --version` runs after
cleanup. No unrelated system temp or user data was swept.

**Next:** major Lantern steady/transient RAM, targeted runtime frame-tail/render
work, then combined gameplay/resource/performance acceptance and issue/report
reconciliation. Home's timed favor pickup replay, remaining Pool favor/throw,
eight-favor reward/restart, pressure and final integrated FPS remain open.
Every next completed Rust phase must preserve matching artifacts outside target,
verify no active target users, clean Cargo and task-owned disposable staging
between runs, and report actual recovery. Retain awake **PID 34744**, original
SYSTEM-only policy/stop mechanism and expiry **2026-10-24 17:57:20 UTC**; no renewal
or stop is needed here. Cleanup does not release awake custody.

## Runtime RAM phase 4 - Windows, 2026-10-24

Lantern Hollow now retains substantially less CPU data and process memory. The
final implementation keeps exact immutable prop-attribute palettes, selects
smaller backend allocator blocks, retires idle GPU texture handles after a new
world commits, evicts unrelated prepared records, and retires completed CPU
preparations when a level load is cancelled during GPU upload. The displayed
world retains its own handles and weak prepared-record reuse. No working-set
trimming, texture/lighting reduction, animation change or map-specific rule is used.

The original control is remote-matching `6700bc16aebd81ef3ffcb203544b046915c837d9`.
All final comparisons use the same preserved nine-level package, SHA-256
`dcfa9bc8f911f7eae2b6cad8f874a8d8a83077559208e02ade1b2a7304bf7b96`.
Lantern's preserved shipped member uses Static/off; it has no Full atlas variant.
High textures/filtering, content, 1280x720 camera route and eight-second warmup
match before/after. Home Full verifies a non-Lantern atlas/circuit case. These
are fresh-process runs with an ordinarily warm filesystem cache, not proof of
cold OS-cache or target-resolution performance. No game/build/bake overlapped
any timing cohort. Host power, GPU, fan and driver settings were not changed.
The recorded custom host limits remain unconfirmed context, not a throttling diagnosis.

All figures below are MiB (2^20 bytes). Private bytes/commit, resident Windows
working set, WDDM dedicated GPU and WDDM shared GPU memory are separate,
overlapping accounting views and must not be added together. Windows working set
is the resident-page measure used here; no Linux RSS measurement is implied.
See Microsoft's [GPU accounting explanation](https://devblogs.microsoft.com/directx/gpus-in-the-task-manager/)
and [heap/private-memory guidance](https://techcommunity.microsoft.com/blog/askperf/an-overview-of-troubleshooting-memory-issues/372673).

| Matched phase | Process private | Working set | Dedicated GPU | Shared GPU |
| --- | ---: | ---: | ---: | ---: |
| Lantern settled (two original controls; final confirmation) | 1964.50 → 1468.37 | 1035.71 → 675.64 | 975.34 → 838.88 | 338.08 → 246.08 |
| Lantern traversal | 1970.73 → 1472.99 | 1040.77 → 680.21 | 975.46 → 838.95 | 339.08 → 247.08 |
| Home Full settled | 912.45 → 714.91 | 503.03 → 443.05 | 461.01 → 324.55 | 209.08 → 149.08 |
| Return to animated menu | 1090.55 → 776.49 | 681.68 → 488.27 | 462.46 → 341.84 | 339.08 → 143.08 |
| Office after menu | 1001.10 → 691.77 | 592.29 → 419.92 | 462.46 → 325.87 | 339.08 → 175.08 |
| Lantern after menu/Office/reload | 1995.59 → 1645.85 | 1068.57 → 725.29 | 975.46 → 967.30 | 339.08 → 271.08 |
| Tenth direct map transition | 2188.30 → 1696.42 | 1256.00 → 777.51 | 975.58 → 967.20 | 466.08 → 278.08 |
| Menu after ten transitions | 1100.35 → 795.01 | 692.73 → 505.20 | 462.33 → 341.71 | 338.08 → 142.08 |

Settled Lantern private memory falls **25.3%**,
and resident working set **34.8%**. The final
single-run kernel peak commit/working set is **1474.01 /
788.03 MiB**; original matched single
controls were 1971.36–1971.82 / 1131.93–1133.19 MiB. For the identical ten-transition
campaign, kernel peak commit/working set changes from
**2466.77 / 1540.43**
to **1795.89 / 1125.80 MiB**.
Process samples are every 200 ms; kernel peak counters additionally capture
short process peaks. WDDM samples do not establish unsampled GPU peak maxima.
Raw samples, losslessly compressed trace/frame CSVs, phase windows and request timings remain
in `task-12/memory-evidence/`; final-run identities carry exact binary hashes.

### What occupies Lantern Hollow

The following are decoded allocation charges/payloads, not subdivisions of total
process RSS. Families are classified from actual model paths; shared image/model
identities are counted once. The unchanged GPU column excludes padding, staging,
texture storage, allocator capacity and driver overhead.

| Content family | CPU prop vertices before → after | Static GPU vertex/index payload |
| --- | ---: | ---: |
| Characters (compiled fallback geometry) | 9.17 → 2.88 | 0.00 |
| Houses and structural props | 7.13 → 1.72 | 9.00 |
| Trees and vegetation | 394.01 → 140.69 | 498.22 |
| Lighting fixture props | 0.58 → 0.13 | 0.73 |
| Rocks, terrain and roads | 3.23 → 0.62 | 4.11 |
| Furniture and other props | 3.24 → 0.66 | 3.94 |

All prop vertices total **417.36 → 146.70 MiB**, preserving every float bit,
signed zero, HDR colour, UV, normal/tangent and atlas selector. Prepared level
records total **462.24 → 191.58 MiB**. Runtime access reconstructs one stack
value; there is no retained expanded float cache. Compiler authoring keeps
editable vectors, so phase 3's bounded export path is preserved. Palettes are
kept only when their measured allocation charge saves at least ten percent.

The largest contributors are **247 trees of three models**:

| Model | Placements | CPU vertices before → after | Static GPU payload |
| --- | ---: | ---: | ---: |
| tree_01 | 62 | 121.32 → 42.23 | 153.87 |
| tree_02 | 61 | 167.26 → 61.86 | 211.25 |
| tree_03 | 124 | 102.65 → 35.52 | 129.55 |

Those three models account for 93.74% of expanded prop CPU vertices and 95.86%
of static prop GPU geometry. Source models and textures are already shared; the
large copies are placed, world-space/baked meshes in spatial batches. GPU shape
payload is still duplicated in this RAM checkpoint. The approved instancing
stage below addresses that remaining structural cost.

Other measured CPU data: prop indices **11.21 MiB**, navigation **19.95 MiB**,
architecture **2.27 MiB**, lighting metadata **2.98 MiB** and compiled collision
records **0.027 MiB**, included in the prepared charge. The collision figure
is serialized/runtime record ownership, not all Rapier, body, pose or query
allocations; those remain in the engine/unassigned heap subtotal. Per-vertex
baked lighting is already included in geometry, not charged again as an atlas.
Lantern has **zero atlas pages, circuit atlas variants, materialized float pages,
probe-field or reflection-capture payloads** in this matched Static member.
Home Full retains **64 MiB** of packed atlas planes, four page records and one
switchable group, with **zero materialized float page copies**. Existing lazy
packed-page behavior is preserved rather than claimed as a new phase 4 saving.

Unique source material/fixture/prop RGBA totals **68.31 MiB across 58 images**:
60.06 MiB material/fixture and 8.25 MiB prop images. Model-cache image counters
overlap those same allocations. Shared source geometry/skin vertices total
**13.24 MiB**; retained encoded GLB input is **zero**. The 21 rendered characters
use **11.64 MiB** of posed GPU vertices and **0.084 MiB** of indices over seven
shared models, separately from static GPU props. Rig/clip metadata and per-actor
state are not falsely included in the static-prop table. One water volume has a
small source surface (4.3 KiB CPU); water shading/attachments are engine resources.
Global decoded audio is **21.04 MiB / 63 clips**, not a particular tree or house.
Luau is about **0.5 MiB**, one module/AI in Lantern, with zero queued frame actions.

First-load native heaps contain **368.84 MiB allocated / 386.87 MiB committed**,
including **18.03 MiB committed slack**. Identified CPU payload charges total
approximately **286.44 MiB**; **82.40 MiB within those heaps** remains unassigned
among engine, physics, pose/rig, native and driver structures. Another
**1081.50 MiB of process private bytes lies outside enumerated heaps**; these
include graphics/native allocations and are not arbitrarily assigned to map
objects. Source model textures report 8.80 MiB with overlap and possible extra
actor images beyond the 68.31 MiB scoped material/fixture/compiled-prop union;
that counter is not added a second time.

The final settled backend allocation breakdown separately identifies **549.19 MiB
static prop buffers** (516.02 MiB requested payload plus allocation padding),
**126.25 MiB material uniforms/internal sampler-index buffers** and **96.75 MiB
sampled texture allocations**. Texture binding helper buffers occupy another
12 MiB. Character bind/pose/skinning, dynamic objects, architecture, environment,
post/decal/backend and staging allocations remain explicit disjoint label groups
in `final-gpu-allocation-breakdown.json`, reconciling to the full allocated total.
The 126.25 MiB binding cost belongs to the engine/backend resource layout, not
505 different tree textures; it is a further measured target for the approved
shared-rendering work. GPU cache texel budgets overlap these allocations and are
not summed again.

### Lifetimes, staging and allocator attribution

The bounded phase 3 archive reader still uses its eight-MiB Zstd history. Lantern
prop reconstruction temporarily overlaps **198.48 MiB** of decoded shared segments
and **326.64 MiB** of encoded prop records. These are transient archive/record
intermediates, not settled level records. The new exact palette builder handles
one bounded batch at a time and releases the batch float vector and attribute
hash maps after conversion. GPU conversion/staging remains bounded to the
existing upload batches and fences. Attribution runs observe about 23 MiB of
backend staging at commit and about 1.875 MiB of retained staging capacity when
settled; the latter is allocator-owned, not a live decoded map page. CPU mip work
is idle, pending install/cache state is clear and scheduled preparation is absent
at settled/unload checkpoints. No resource is freed by bypassing a GPU fence.

The backend MemoryUsage policy reduces initial reserved block capacity from
**1152 to 924 MiB** in the controlled arena-layout experiment, with identical
allocated live payload. Its approximately **226 MiB process-commit** effect is
separate from the approximately **271 MiB CPU vertex** saving. These are mechanism
comparisons, not additive independent accounting totals. The default policy
change uses wgpu's supported per-device allocation hints, not global driver or
host tuning. Old-policy diagnostic controls are explicitly excluded from FPS claims.

Cache-only GPU sheets are pruned after replacement commit, preserving active
draw/circuit banks and native queue owners. In the menu/Office/reload control,
pruning removes 119.68 MiB of otherwise idle texture payload and about 65 MiB of
extra process commit on the final Lantern visit. Optional source-image caching
remains bounded at 256 entries/256 MiB; diagnostic worker-image reports distinguish
cache-only pixels from live material owners. Unrelated prepared identities are
evicted: the original direct-transition control retains Office's prepared/build
owner while Lantern is active; final candidate checkpoints retain none. The old
control therefore fails the candidate-only zero-owner invariant, documented in
`baseline-retention-assessment.json`; its successful native timing samples remain valid.

The pre-correction C2 RAM candidate left completed CPU caches after GPU-stage
cancellation despite no worker
being active. Serial worker maintenance now retires them after pending resources
are dropped, retaining only weak identities for the displayed rollback world.
The matched GPU-cancelled Office checkpoint (C2 before the correction versus
final binary, rather than the original release control) changes from **859.10 / 642.68** to **592.85 / 378.11 MiB private/working set**. Reopening the displayed Office proves a prepared-record cache hit after retirement; Lantern recovery, same-level reload and final menu unload pass.

The cancellation/same-level-reload stress path still reaches **2242.91 MiB kernel
peak commit / 1010.25 MiB peak working set**. After same-level reload it settles
at **2072.19 MiB private / 799.38 MiB working set**, with **1223.76 MiB dedicated /
310.08 MiB shared GPU**. This residual is not hidden: rollback-safe replacement
uploads duplicate large GPU geometry, and the backend retains **1372.00 MiB
reserved against 851.44 MiB allocated**, including **520.56 MiB unused capacity**.
All six process heaps report **398.95 MiB allocated / 447.59 MiB committed** there.
A separately observed **28 MiB cache-only source-image** allocation is bounded
and cleared on menu unload. After menu return the stress process is
**992.76 / 577.26 MiB private/working set**: heaps are **272.11 allocated /
372.03 committed**, backend **270.56 allocated / 452.00 reserved**, and idle GPU
texture payload is zero. These views overlap; they are evidence of retained
allocator/driver capacity, not quantities to sum or a full attribution of every
process byte. Reducing the remaining GPU shape duplication is the approved next
stage; this checkpoint does not promise every reload stays below 2 GiB commit.

Every final settled witness for an inactive distinct level reports zero prepared-
record, build and transport owners; pending CPU work/install is absent. Same-
identity reloads can share records with the active world; after final menu unload
their retired owners are zero. Menu measurements include
the live animated menu and its approximately 97.48 MiB of prepared records, not
an empty renderer. Repeated process/driver totals can remain above first-load
levels after ownership clears. Win32 HeapSummary reports all six process heaps,
separating allocated bytes from committed slack; backend allocation reports
separate live labeled allocations from unused block capacity. These prove
allocator/driver residency and bounded caches rather than treating every elevated
working-set page as a map leak. HeapSummary includes native/driver heaps and
excludes allocations made outside them; the remaining process subtotal is kept
unassigned, not arbitrarily charged to map objects or declared wholly GPU memory.
Detailed per-phase heap/block/label figures remain in the final analysis and
allocator receipts. No heap compaction, empty-working-set or residency trimming is used.

The shared prop-buffer arena prototype was **rejected**. It removed 32.5 MiB of
HAL allocation padding and 780 buffer objects, but initial process/GPU totals
were nearly unchanged and the ten-transition run finished at 1826.19 MiB private
versus 1703.08 MiB with individual buffers; menu retention was also worse. The
final binary retains individual batch buffers. Prototype source diff and controls
are preserved so lower live allocation is not misreported as lower process residency.

### Timing and validation

Original Lantern: startup to request **3.927 s**, request to present **4.772 s**; settled mean/p95/p99 loop **13.533/16.078/16.336 ms**, traversal **6.715/15.745/16.837 ms**.

Final Lantern: startup to request **5.038 s**, request to present **5.909 s**; settled mean/p95/p99 loop **13.457/16.136/16.374 ms**, traversal **6.740/15.384/16.693 ms**.

Original Home Full: startup to request **3.936 s**, request to present **3.178 s**; settled mean/p95/p99 loop **3.258/5.812/6.648 ms**, traversal **2.516/4.977/5.865 ms**.

Final Home Full: startup to request **4.026 s**, request to present **3.195 s**; settled mean/p95/p99 loop **3.075/5.559/6.367 ms**, traversal **2.468/4.736/6.262 ms**.

The main tradeoff is roughly **1.1 s** extra Lantern preparation, dominated by
prop parsing/attribute interning (about 0.37 to 1.30 s), in exchange for substantial
settled/peak savings. Home has a much smaller parsing cost. FPS gains are not
claimed. Every throughput cohort requested and applied Immediate on DX12,
1280x720 physical drawable, swap interval zero, VSync/limiters off, no fixed delta
or capture. Home runs well above the reported 165 Hz desktop refresh; Lantern
frame distributions do not plateau at refresh. Capture/fixed-step, allocation
report and diagnostic lifecycle runs are excluded from throughput conclusions.

Final source is frozen across successful format, strict workspace/all-targets/
all-features Clippy, **308 Rust tests (zero failed/ignored)** and locked release
workspace/all-features build. The two small new regressions protect exact HDR/
signed-zero attributes, copy-on-write independence and the u16 palette boundary;
they use bounded vectors, not map bakes. Asset/prop checks pass; texture checks
pass with 100 existing advisories. All 24 targeted Python checks pass. Broad
Python runs 369 tests with the same eight failures, nine errors and 16 skips;
new/resolved failure-header sets are empty. No full shipped map was rebaked.
Phase 3 encoder/storage/bake gains remain historical and are not claimed anew.

Final native checks cover matched Lantern street/campfire/pond, Home Full on/off,
Pool water and Beach views, all 16 Arcade Full/Static UV/restroom circuit states,
ordinary Shape/One Guest routes/restarts, held props, water, blood, mirrors and
cycles, five animated skins/ragdolls/reset and stairs/ramp/crouch. Movement
checks retain all **17 exact trajectories / 4,080 ticks**. The street's 12
one-step sky-pixel differences also occur on an unchanged-control repeat; other
matched views and circuit states are exact. The final street also matches the
unchanged-control repeat byte-for-byte. Raw images are untouched; an inherited
overview pose was excluded for poor framing and corrected scene labels are used.
No gameplay acceptance is inferred from fixed-step capture FPS. macOS/Linux
runtime behavior remains unverified in this Windows phase.

Home's timed favor-pickup replay, remaining Pool favor/throw, eight-favor
reward/restart, new pressure campaign and final integrated FPS acceptance remain
open. Diagnostic carry checks do not resolve those ordinary-input acceptance items.

### Approved shared geometry and automatic instancing stage

Connor explicitly approved the feature after this RAM assessment. It is the next
dedicated stage, before general frame-tail work. The three trees have 4,883,797
placed vertices/5,388,384 indices but only 68,989 source vertices/77,106 source
indices. Counts exactly match source topology times placements; this vertex-lit
path transforms geometry and samples per-placement HDR lighting without bake
subdivision/displacement. Full atlas paths may split triangle corners and assign
independent chart coordinates, so canonical topology must follow the actual route.

A conservative shared-tree GPU budget is **6.99 MiB canonical geometry +
74.52 MiB independent RGBA32F lighting + 0.03 MiB instance records = 81.54 MiB**,
versus the current **494.66 MiB** tree GPU payload. This is an arithmetic design
budget, not an implemented or measured saving. Lighting is independent, but not
necessarily incompressible: exact palettes can retain less than full RGBA32F.
The final attribute inventory measures **51.51 MiB independent baked colour**,
**88.06 MiB position/UV/frame streams** and **10.28 MiB indices** for these trees.
Keeping those exact compressed lighting streams with canonical CPU geometry
yields an estimated **57.21 MiB payload** before metadata; the optional GPU
palette form needs validation of its alignment/fetch cost. These remain design
budgets, not implemented savings.
A conservative full-RGBA CPU representation is about 80.22 MiB before metadata,
versus about 149.89 MiB of current placed tree vertices/indices; source-model
cache ownership is separate and must not be counted twice. Native CPU/GPU
transform rounding must be checked before promising pixel-exact output.

Implement shared storage separately from merging instances into fewer draws.
Start with compatible repeated static opaque/cutout props, then validate furniture,
lamps and repeated physics props automatically without editor flags. Preserve
per-instance bounds/culling, transforms including scale/rotation, baked-lighting
attributes, material/emission/circuit state and source identity. Physics bodies
and interactions remain independent; skinned characters keep independent poses;
ragdolls, sorted transparency and uniquely modified topology keep suitable paths.
If records change, retain bounded legacy compatibility and validation. Benchmark
against this final RAM candidate and original control; test appearance/shadows,
variants, individual culling, pickup/motion, cancellation, unload/reload and
backend portability. Do not reuse one tree's baked colours for every tree.

Remaining order after this completed RAM phase: **shared geometry/automatic
instancing**, then combined target-resolution Full runtime/frame-tail work
(including the roughly 198 ms item installation/spawn_runtime_props block and
54/28 ms prop/decal uploads), then incremental authored Environment preview
(14.266 s worker/16.222 s request-to-ready baseline), then conditional cold
renderer/pipeline bootstrap investigation, then final combined acceptance and
issue/report reconciliation. Physics preparation already runs on the worker;
the 198 ms block must not be assumed to be Rapier construction. Preserve existing
character-pipeline prewarming. No extra broad compiler, portal, GPU-solver or codec
rewrite is part of this queue.

The final 21 executable/DLL/symbol files, matched packages/recovery sources and
current comparison media are preserved/hash-checked outside `target`. Evidence is
indexed by `local/windows-validation-0.13.0/memory-phase4/evidence-index.json`;
the canonical writer/commit/remote handoff remains
`local/windows-validation-0.13.0/final/ownership-handoff.json`. No executable
Library bundle was uploaded or raw video recorded. Task-owned staging/cache/temp
files are removed. Verified Cargo cleanup exits zero, recovering
**10,135,814,144 actual bytes (9.44 GiB)**;
free space afterward is **87,405,518,848 bytes
(81.40 GiB)**. These are measured filesystem
deltas, separate from Cargo's logical deletion count. Task-owned disposable cleanup recovers an additional **34,709,504 bytes**,
leaving **87,440,224,256 bytes (81.44 GiB)** free.
Preserved hashes and compiler
version remain usable after cleanup. Awake PID **34744** remains in custody under
its existing SYSTEM-only policy/stop mechanism, expiry **2026-10-24 17:57:20 UTC**.
Cleanup does not stop it or release that custody.

The 95 verbose trace/CSV files are losslessly compressed from **737,842,126 to 72,697,834 bytes**. Every decompressed SHA-256 is checked before removing its raw file; saved-evidence readers accept the .gz form. This recovers **664,576,000 additional actual bytes**, with **88,103,923,712 bytes (82.05 GiB)** free at that checkpoint. PNGs, binaries, settings, samples and comparison summaries remain directly accessible.


### Shared canonical prop geometry and automatic instancing — completed Windows stage

Implemented in `7965ded350e2b5c6523aac510ac00e6fa95b3be0`, against the accepted
RAM checkpoint `0b6de58ebfc29b7c271c12bccac77aab246c8690`. This section supersedes
the earlier proposal with actual inventory and native measurements. The final
player SHA-256 is `caa80df645c3a994f701da5cc127fe5b78f353bbb75eda418e48c7a144b4584b`; the RAM control is
`f534960f9e1319b14a671bd7afcc27252961fe4868ce70ba52ad245f126b9174`. Exact source hashes, commands, binaries,
settings, samples, original PNGs and compact evidence are preserved under
`C:\Users\Connor Dawkins\Documents\Codex\2026-10-24\task\instancing-evidence\final`.
The local index is `local/windows-validation-0.13.0/instancing/evidence-index.json`.
Earlier pre-compatibility cohorts remain separately labeled and are excluded from
the final tables below.

The existing bounded decoder remains authoritative. Every position, normal, UV,
frame, neutral albedo, lightmap field and ordered index must match the verified
source under its authored transform exactly before CPU storage is factored.
The source model allocation is reused; exact independent HDR colour palettes
remain per placed vertex. Mutation detaches through copy-on-write. GPU storage
then uses source geometry, transforms and separate RGBA32F lighting/required UV
visibility. Compatible material/controller ranges merge automatically; each
placement keeps individual culling. The route also covers emission, reflection
and flashlight shadows. No map-author flag, format change, quality reduction,
texture replacement or full shipped-map bake is involved. On-disk geometry stays
in its existing expanded format; decoding still determines the load peak.

Repeated furniture and lamps benefit through the same generic admission. Rigid
props already shared source meshes; their new automatic draw grouping additionally
requires bit-exact fragment lighting, emission and culling parity. Independent
probes or effects retain separate draws. Skinned poses, ragdolls, sorted blends,
atlas-split topology and altered/unknown geometry retain their appropriate paths.
Texture/uniform variants can share; variants changing per-vertex factors fall back.
Home Full's split chart data remains independent and shows no meaningful saving.
Six-group devices use the instance route. Five-group devices keep CPU sharing and
expanded GPU uploads; unused instance declarations are excluded from their shader
module. Oversized per-source storage uses the same fallback. The accepted allocator
policy, exact palettes and cancelled-cache retirement are preserved; no buffer arena
was reintroduced.

**Actual geometry/lighting inventory.** All 247 trees are admitted: 4,883,797 placed
vertices factor to 68,989 source vertices. CPU retained tree vertices fall from
146,397,820 to 54,068,734 bytes (**139.62 →51.56 MiB**), while the independent baked
colour heap remains exactly **54,011,630 bytes /51.51 MiB**. CPU index payload remains
10,776,768 bytes /10.28 MiB. Existing source-cache geometry is already owned and is
not charged again. Requested tree GPU payload, reconciled from inventory with the
native six-group upload, is **518,691,656 →85,509,388 bytes /494.66 →81.55 MiB**:
7,329,068 shape bytes +78,140,752 independent RGBA32F bytes +39,520 transform bytes
+48 dummy visibility bytes. This is a requested payload, not driver residency.

Across Lantern props, the native upload reports **48 canonical meshes /1,068
placements**, including repeated chairs/furniture, 10 standing lamps, 8 wall lamps
and 5 streetlights. Requested prop buffers fall **541,085,240 →92,803,072 bytes
(516.02 →88.50 MiB)**. The latter contains 11,015,264 vertex bytes, 261,072 index bytes
and 81,526,736 independent lighting/instance bytes. Prepared CPU retained-build bytes
fall 200,888,446 →105,937,404. The existing model-cache geometry/skin allocation stays
13,881,280 bytes. Settled HAL live buffers are 759,627,776 →187,367,424 bytes; texture
bytes remain 132,644,864. These are overlapping views of resources: never add
requested bytes, HAL counters, allocator reservations, process memory and WDDM usage.

**Matched process memory.** Values are settled medians in MiB, private /resident.
Fresh runs warm 8 seconds; samples are collected sequentially with no build or bake
overlapping a timing window. The final current-source cohort is used throughout.

| Scene/state | Accepted RAM | Canonical |
|---|---:|---:|
| Lantern fresh, repeat 1 | 1464.64 / 674.77 | 866.08 / 517.67 |
| Lantern fresh, repeat 2 | 1463.94 / 674.93 | 863.14 / 518.15 |
| Home Full,720p | 717.32 / 448.33 | 715.93 / 446.12 |
| Menu after first unload | 793.37 / 508.15 | 818.38 / 530.07 |
| Office then Lantern | 1648.47 / 728.50 | 945.67 / 534.68 |
| Same-level reload after cancellation/rollback | 2160.57 / 795.48 | 1034.78 / 563.53 |
| Tenth direct transition | 1701.14 / 782.56 | 1124.54 / 653.92 |
| Menu after tenth transition | 867.61 / 509.70 | 898.44 / 546.11 |
| Lantern,1440p | 1627.84 / 738.71 | 966.64 / 581.99 |
| Home Full,1440p | 811.14 / 506.21 | 811.81 / 506.88 |

The two 720p Lantern runs average **1464.29 /674.85 →864.61 /517.91 MiB**.
WDDM dedicated/shared usage is **838.88 /246.08 →390.07 /182.08 MiB**. Fresh-process
peaks across load through traversal are at most **1471.12 /791.70 →868.43 /789.69 MiB**;
peak resident memory is essentially unchanged because legacy decoding expands first.
After the matched same-level reload, WDDM dedicated/shared is 1223.75 /302.08 →518.30
/174.08 MiB. After transition 10 it is 967.20 /278.08 →518.32 /182.08 MiB.
Menu-after-unload private/resident is modestly higher in the candidate, as the table
shows; the feature does not improve every state.

No retired owners or scheduled CPU work remain in cancellation, rollback, unload
or transition witnesses. Residual retention remains: the reloaded Lantern backend
has **851.44 /1364.00 →305.62 /532.00 MiB allocated /reserved**. The candidate still
has 226.38 MiB unused capacity, and its reload/transition process footprint remains
above a fresh load. This distinguishes live resources and capacity without claiming
that all remaining private/resident memory has been attributed or eliminated.

**Request to first presented scene, seconds.** This is a warm local native campaign,
not an OS-cache-cold benchmark. Lantern's 720p pair averages 5.8432 →5.5275 seconds;
CPU preparation is slightly slower while the smaller GPU installation reduces net
load time. Home Full has no general load-time gain.

| Scene | Accepted RAM | Canonical |
|---|---:|---:|
| Lantern 720p,R1 | 5.8969 | 5.5007 |
| Lantern 720p,R2 | 5.7895 | 5.5543 |
| HomeFull 720p | 3.1526 | 3.2993 |
| Lantern 1440p | 6.2958 | 5.6716 |
| HomeFull 1440p | 3.3578 | 3.4687 |

**Uncapped frame distribution, milliseconds.** Both 1280×720 windowed and 2560×1440
fullscreen runs record actual Immediate presentation and physical drawable size,
VSync off, no frame limiter, no fixed delta and no capture during timing. Home and
campfire rates exceed the 165 Hz display reference, and GPU-heavy street windows
track GPU elapsed time rather than plateauing at refresh. Lantern requests Static
but the existing package resolves to its vertex-lit **Off/low** path in both builds;
High graphics/filtering and bloom remain identical. Home resolves **Full lighting
and reflections**. Requested settings alone are not presented as actual quality.

| Scene/window | Mean before→after | Median | p 95 | p 99 |
|---|---:|---:|---:|---:|
| Lantern 720p,R1/idle | 13.729 → 13.771 | 14.401 → 14.332 | 16.094 → 15.958 | 17.489 → 17.214 |
| Lantern 720p,R1/traversed | 6.860 → 6.828 | 5.699 → 5.591 | 15.429 → 15.143 | 16.644 → 16.303 |
| Lantern 720p,R2/idle | 13.833 → 13.897 | 14.399 → 14.352 | 16.132 → 16.016 | 16.706 → 16.856 |
| Lantern 720p,R2/traversed | 6.507 → 6.862 | 5.117 → 5.621 | 15.588 → 15.156 | 17.276 → 16.478 |
| HomeFull 720p/idle | 3.381 → 3.265 | 3.937 → 3.911 | 5.942 → 5.709 | 6.825 → 6.381 |
| HomeFull 720p/traversed | 2.524 → 2.538 | 1.579 → 1.668 | 4.947 → 5.170 | 6.903 → 5.904 |
| Lantern 1440p/idle | 18.126 → 18.674 | 18.082 → 18.505 | 19.826 → 20.998 | 20.537 → 22.151 |
| Lantern 1440p/traversed | 13.974 → 14.238 | 15.550 → 16.120 | 20.358 → 19.989 | 24.069 → 25.405 |
| HomeFull 1440p/idle | 13.375 → 13.354 | 9.316 → 8.389 | 29.488 → 29.792 | 30.648 → 30.353 |
| HomeFull 1440p/traversed | 9.329 → 9.235 | 9.028 → 9.348 | 22.383 → 19.254 | 26.244 → 26.534 |

No FPS improvement is claimed. The 720p traversal pair means average 6.683 →6.845 ms
(+2.4%); one repeat is slightly faster and one slower. The single 1440p Lantern pair
also has higher mean/p 99 values, as retained above. Target-resolution results do
not close the later combined Full-quality frame-tail acceptance.

**Three counterbalanced fixed-camera repeats** separate storage from merging.
Expanded GPU forces the existing five-group route on the same adapter; shared
storage keeps source buffers but disables static draw merging. GPU timestamps are
deduplicated by sample ID. CPU renderer-call wall time includes command work and
waits, and is not a pure submission microbenchmark.

| Mode/pose | Mean loop ms | GPU elapsed ms | Renderer wall ms | Actual draws |
|---|---:|---:|---:|---:|
| ram/street | 16.151 | 15.543 | 2.477 | 853 |
| ram/campfire | 4.948 | 4.063 | 0.842 | 75 |
| expanded-gpu/street | 16.122 | 15.513 | 2.162 | 853 |
| expanded-gpu/campfire | 4.956 | 4.083 | 0.817 | 75 |
| shared-storage/street | 16.229 | 15.556 | 2.565 | 1562 |
| shared-storage/campfire | 4.997 | 4.200 | 0.933 | 140 |
| shared-merged/street | 16.148 | 15.449 | 1.863 | 478 |
| shared-merged/campfire | 5.140 | 4.407 | 0.830 | 70 |

Street mean is unchanged (16.151 →16.148 ms), while renderer-call wall time drops
about 25% and draws 853 →478. Campfire is **4.949 →5.140 ms (+3.9%)**, slower in all
three merged repeats. Shared storage alone averages 4.997 ms (+1.0%); grouping adds
about 2.9% there. These controls implicate grouping/order/GPU batch behavior in much
of that scene's cost; they do not isolate overdraw, cache effects or a particular
transform/lighting instruction. This is a retained memory/CPU-submission tradeoff,
not a free throughput gain. The raw controls remain available for the next runtime
stage.

**Validation and visual bounds.** Final fmt, strict workspace/all-target/all-feature
Clippy,312 Rust tests and locked workspace/all-feature release all exit 0 against a
frozen source set. Four new tiny regressions protect exact HDR/signed-zero data,
source/topology matching, COW independence and rigid lighting equality; no Rust
test bakes a shipped map. Naga validates 11 legacy and 12 instanced entry points.
Windows native DX12 on an RTX 3070 Laptop GPU is the actual backend coverage;
forcing five groups on this adapter is not a second hardware/backend test.
macOS/Linux runtime behavior remains untested.

The furniture fixture checks scale/yaw/pitch/roll, a material variant, lamps with
transparent fallback,12 distinct pickup bodies, rotation/release and individual
culling. Ten unaffected body poses remain exact; visible vertices match and draws
fall 24 →14. Framed editor lighting preview, export integrity and require-current
verification pass. The copied fixture was correctly marked stale after the shader
change and rebuilt before these final checks. Existing legacy shipped payloads
load without a wire-format migration. The initial five-group shader failure was
fixed and the final fallback has pixel-exact street output.

Raw native Lantern street/campfire/pond views preserve lighting, shapes and shadows;
only **1 /1 /6 pixels** change, maximum RGB channel error **3 /1 /1** out of 255.
Coordinates and untouched source PNG hashes are recorded in `visual-review.json`.
Home Full on/off, Pool water and Beach are exact; all 16 Arcade Full/Static circuit
states are exact against the accepted RAM reference. Ordinary Shape/One Guest
routes/restart, Home/Pool/Lab/blood/mirror/cycles, five independent skins/ragdolls/
reset and stairs/ramp/crouch pass. All 17 movement trajectories /4,080 ticks remain
byte-exact. Capture runs make no FPS claim.

Asset/prop/texture checks and 24 targeted Python checks pass; 100 texture advisories
remain historical. The broad 369-check Python run retains the same 8 failures,
9 errors and 16 skips with no changed failure headers. Its unchanged wire-codec
control uses the preserved RAM repacker; current Rust package tests and current
compiler/runtime fixture checks are recorded separately.

A final evidence-helper import mistakenly selected the prior task's unguarded
`gates.py`, rerunning cached tests/release after native timing had finished and
overwriting five historical **unprefixed scratch gate records**. Their original
contents were not recoverable from available copies. Accepted `c3-*` RAM gate
evidence, baseline binaries and native measurements are intact and hash-checked;
the import now uses the exact current file. This preservation limitation is
recorded in `final/import-path-incident.json` and the old folder's
`unprefixed-gate-files-provenance.json`; the altered unprefixed records must not be
used as RAM-baseline evidence.

**Cleanup and handoff.** All 21 final executables/DLLs/symbol files are preserved
and hash-checked outside target. Cargo removes 7,814 files /10.6 GiB logical, recovering
**9,586,139,136 actual bytes (8.93 GiB)**.
Completed traces are losslessly compressed from **951,700,069 to
83,374,407 bytes**, with every decompressed SHA checked before
raw removal; observed free-space recovery is 868,450,304 bytes.
Task-owned disposable cleanup recovers 56,553,472 further
bytes, leaving **84,190,552,064 bytes (78.41 GiB)**
free at that checkpoint. Raw PNGs, source fixtures and compact evidence remain.
No raw video or executable Library upload was made. Awake PID 34744 remains active
under its existing SYSTEM-only policy, expiry 2026-10-24 17:57:20 UTC; custody continues
for the whole queue. No host power/fan/driver/security settings were changed.

Remaining order: combined Full-quality target-resolution runtime/frame-tail work
(including prop activation/upload blocks and GPU waits), incremental Environment
preview preparation, conditional cold renderer/pipeline bootstrap work, then final
combined acceptance, issue reconciliation and cleanup. Home favor acceptance,
Pool favor/throw, eight-favor reward/restart and the new pressure campaign remain
open. Diagnostic carry and this feature's timing checks do not close them.
The final canonical ownership handoff records the exact pushed tip/remote and
explicit writer release after final verification; no main merge, release, version
bump or GitHub CI wait is part of this stage.


## Combined runtime rendering and frame-tail phase (2026-10-24)

**Scope and evidence.** Baseline is remote-matching `a8eff05b10a32bb7817fb1f70f7cecd8ece9a0c3` on
`0.13.0-staging`. This phase preserves the prior canonical geometry sharing, independent
placement colours, copy-on-write and limited-bind-group fallback. No map, visual asset,
lighting quality, release version, main merge or host setting is changed. The evidence
index is `local/windows-validation-0.13.0/runtime-frame-tail/evidence-index.json`; its
absolute task root is `C:\Users\Connor Dawkins\Documents\Codex\2026-10-24\task-3\runtime-evidence`. Accepted C3 RAM evidence and prior binaries remain intact;
the earlier five overwritten historical unprefixed scratch gates remain unrecovered.

**Attribution before changes.** Labelled GPU timestamps and CPU render stages at the
actual 2560×1440 drawable identify scene rendering as the main warmed Lantern cost
(about 19.5 ms GPU in the initial fixed view). Arcade's scene was about 6.6 ms plus
1.8–2.6 ms planar reflection; sampled CPU submission was about 1.6–1.8 ms for
Arcade/Lantern and 0.5 ms for Pool/Home. Bloom/UI were small. These samples did not
support reducing lighting, shadows, reflection quality or scene content. Physical
world preparation was already off-thread: the measured approximately 199.6 ms Arcade
activation comprised 0.95 ms neutral spawning and 198.6 ms dynamic GPU setup, dominated
by mesh sheets/content keys/mips (179.1 ms), then objects (18.0 ms) and instances
(1.2 ms). It was not 199 ms of Rapier world creation.

**Retained implementation.** Prop source vertices, independent colours, UV receiver
work, indices and primitive bounds now advance within batches: 4096 geometry/index
items or 256 reactive vertices per unit, with one shared four-millisecond deadline
across batches. Large decals are CPU-packed on the existing preparation worker and
uploaded in bounded 4096-element slices. Real PNG sheets use the existing bounded mip
worker; Static banks share the same uploaded sheet handles. One process-wide mip job
prevents cancelled owners from overlapping scratch preparation; auxiliary batches
contain at most two fitted sheets (about 11 MiB). Static-sheet content hashing and
mips overlap bounded prop geometry/bounds work, instead of serializing all static
sheets in Auxiliary. Weak immutable image identities
memoize unchanged content keys without owning decoded pixels and follow editor
copy-on-write replacements. Atomic gameplay publication still waits for all resources.

Only actual physical-item sheets are held across renderer publication/cache eviction
and runtime-prop installation, then released immediately. CPU-only/cache-hit steps
skip needless GPU fences. At most two bounded upload units are in flight, callbacks
own only completion flags, and consumed Static-family boundaries plus the final
transaction receive fences. Cancellation drops pending owners without joining a mip
worker. Review caught a two-source job incorrectly popping four sources; C9 consumes
the exact submitted count. Earlier C8 load measurements are preserved as rejected
incomplete-prewarm evidence, not final acceptance.

Static and rigid instance vertex paths are specialized so static draws avoid the
unneeded runtime lighting branch. Rigid groups admit opaque, single-sided meshes
with determinate culling parity; cutout, two-sided or indeterminate cases retain
ordinary draws with shared source storage. Original 104-byte full-precision shape
input and independent lighting data remain. No Lantern-specific exception is added.

**Measured loading and responsiveness.** Three matched fresh-process loads per side
for each level; one longer timing run and two short loading-only runs. The short
repeats enable matched GPU resource diagnostics on both sides; exact flags and
probe overhead remain visible per run. Local filesystem
caches are unspecified, so these are not claimed to be cold disk measurements. Ready
is request to first scene presentation; all values below are medians except mean
process peak commit/RSS. Handler values are medians of each run's worst handler, not
per-frame p95. Full raw ranges and per-handler samples remain in `final10/accepted10-comparison.json`.

| Level | Ready seconds before → after | Item activation ms | Worst prop handler ms | Worst decal handler ms | Peak commit/RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| arcade | 6.389 -> 6.352 | 220.2 -> 32.0 | 63.1 -> 6.3 | 28.3 -> 0.5 | 1861.0/1087.8 -> 1829.5/1056.1 |
| pool | 6.417 -> 6.425 | 33.0 -> 9.9 | 51.4 -> 5.4 | 63.1 -> 1.3 | 993.4/586.6 -> 994.6/588.1 |
| lantern_hollow | 5.782 -> 5.859 | 18.1 -> 7.9 | 17.4 -> 7.9 | 13.1 -> 0.4 | 966.5/822.1 -> 964.3/822.8 |

A four-millisecond CPU budget is not a hard driver-allocation deadline: indivisible
buffer/texture allocation or a queued write can still exceed it. Remaining atomic
activation, total-ready differences and process-RAM tradeoffs are visible in the
table; this phase does not declare a zero-regression readiness/RAM gate by inference
from shorter handlers. Resource ownership, pending mip work and item-sheet holds
are checked separately after load/cancel/unload. Lantern median readiness remains
about 77 ms slower, Pool about 8 ms slower; Pool mean peak commit is about 1.2 MiB
higher. The strict zero-regression readiness/RAM gate is not closed. The four-ms
budget is bounded work rather than a hard real-time promise (worst final prop
handler 8.6 ms).

**Rendering and frame tails.** All throughput timing uses VSync off, no frame
limiter, no fixed simulation delta, no recorder, verified Immediate presentation
and physical 2560×1440 drawable. Fixed views warm for at least five seconds; camera
routes use identical scripted keyboard/mouse actions and are reported separately.
Capture runs are separate. Full Arcade, menu, Pool and Home use actual Full lighting;
Lantern and Office packages provide Off lighting, which is reported as actual rather
than being mislabeled Full. The user confirmed the laptop is intentionally limited
by its charger. No further clock/power diagnosis or host changes followed that
clarification. Final pairs below are descriptive, one bounded pair per scenario;
they do not establish unrestricted RTX 3070 throughput or final integrated FPS
acceptance. Values are mean/p95/p99 frame milliseconds, lower is better.

| Scenario / window | Baseline ms | Final ms |
| --- | ---: | ---: |
| arcade/fixed | 12.03/17.72/22.83 | 12.06/18.81/23.39 |
| arcade/route | 7.88/11.34/13.66 | 7.99/11.44/14.43 |
| home/fixed | 12.93/29.59/32.75 | 13.05/29.30/33.74 |
| home/route | 8.15/14.96/21.23 | 8.21/14.42/22.48 |
| lantern_hollow/fixed | 21.45/26.35/30.78 | 20.18/22.67/23.51 |
| lantern_hollow/campfire | 12.50/18.32/18.95 | 12.48/18.26/19.00 |
| lantern_hollow/route | 12.42/18.65/19.44 | 12.34/18.40/20.31 |
| menu/fixed | 12.92/25.55/31.35 | 12.88/25.03/31.97 |
| office/fixed | 9.76/16.47/19.19 | 9.68/16.41/19.25 |
| office/route | 6.33/10.53/16.87 | 6.37/10.35/15.23 |
| pool/fixed | 10.15/17.90/21.76 | 10.06/17.73/22.26 |
| pool/route | 8.41/13.09/18.45 | 8.19/12.88/16.96 |

The original repeated campfire instancing tradeoff remains open. Shared storage,
merged draws and expanded geometry were controlled independently. In three matched
720p controls, static/rigid specialization improved the street mean from about
16.09 to 14.30 ms with the same 478 draws; late campfire was approximately 4.91
versus 4.95 ms. That is directional support for specialization, not proof that
merging fixed campfire. No FPS gain is inferred from RAM savings.

**Rejected prototypes.** Depth ordering, canonical sidedness culling, size cutoffs,
static opaque-only merge filter, CPU fog preparation and identity-transform elision gave
mixed/worse results and were removed. Compact shape input plus transform elision
made street much faster but regressed repeated late campfire mean by about 6.6%;
it was removed, including its temporary ABI test. Early camera-transition windows
and late warmed windows remain separate. The old buffer arena and combined compute
submission experiments were not repeated. Automatic approval review rejected a
proposed two-to-four-sheet auxiliary batch because it could increase peak memory;
it was not executed. The retained overlap path preserves the two-sheet bound.

**Validation.** C10 strict workspace/all-target/all-feature Clippy with warnings
denied, locked release workspace/all-feature build and 314 Rust tests pass. The two
new tiny owner tests protect incremental culling bounds/count/fallback and weak
content-key copy-on-write/lifetime behavior; neither bakes maps or requires a GPU.
Final Windows DX12 native checks cover Full reflections, water/physics props, blood,
five skins and ragdolls, material variants, nonuniform/reflected props, culling,
forced five-bind-group fallback, CPU and GPU cancellation, load/reload, ten
alternating transitions and return to menu. Retired noncurrent owners and pending
work return to zero. Historical weak probes may alias the active cached record on
same-level reload; all retired probes reach zero after return to menu. Seventeen Portal1 movement replays (4080 ticks) compare byte-exact
against the accepted player, plus native crouch/ramp/stairs checks. Full and Static
circuit sequences compare all eight captured states. Editor environment preview,
export/current verification and real Build/Play return use the isolated small fixture.
Numeric visual comparisons and native captures are retained; no FPS claim comes
from fixed-delta correctness runs.

Asset/prop/texture checks and 24 targeted Python checks pass. Broad Python retains
the same historical 8 failures, 9 errors and 16 skips across 369 checks, with identical
failure headers; the unchanged wire-codec control uses the preserved RAM repacker,
and current Rust/compiler/native checks are separate. Current tooling resolves
runner modules by exact file path and never imports historical gate entry points.

**Delivery and remaining work.** The user cancelled the comparison video before
any comparison video capture, montage or Library upload began. Native validation captures
remain as evidence. No media or executable upload was made. Cleanup and final
exact-tip remote verification are recorded in the final ownership handoff.
The awake helper remains in custody for the whole queue under its existing bounded
SYSTEM-only policy. No Mac/Linux native coverage is claimed. Home/Pool favor,
reward/restart, pressure and final integrated FPS acceptance remain open. Next is
incremental Environment preview preparation, conditional cold renderer/pipeline
bootstrap only if still measured, then final combined acceptance/issue reconciliation. Final packaging also includes
the requested shareable Windows ZIP: game EXE, required runtime/assets, modes and
demo levels; validate launch and discovery in a clean extraction outside the repo
without Rust/Python/editor dependencies, excluding logs/user data/symbols/developer
tools. This is queued, not yet built or validated; no friend contact, public release,
main merge or version bump is authorized.


**Runtime-phase cleanup.** All 21 selected executables/DLLs/symbols are preserved
and re-hash-checked outside target. A post-clean native player/DLL load succeeds;
repository assets are still used, so this does not validate the queued portable ZIP.
Cargo removes 8,223 files / 11.3 GiB logical and recovers 10104889344
actual bytes. Trace compression reduces 4051584111 to 253526387 bytes;
large derived summaries/logs reduce 707046259 to 37652168 bytes.
Every decompressed SHA is checked before raw removal. Superseded task-owned binary
payloads are removed with prior hashes recorded; accepted baseline payloads and
unique native captures remain. At this checkpoint 77488578560 bytes are free.
Awake PID 34744 remains active, existing expiry 2026-10-24 17:57:20 UTC; custody
continues for the queue. No main merge, version bump, release or CI wait occurs.

## Incremental authored Environment preview (Windows, 2026-10-24)

**Result.** Normal nonoccluding prop edits now rebuild affected batches while
sharing unchanged immutable prop storage and architecture. The real editor
previously spent about 13.2-13.4 s resolving Lantern prop vertices after each
change; fresh lighting cost about 0.10 s and architecture about 0.98 s. The new
path still bakes the lighting field on changed requests. It compares its complete
sampling record before sharing any Environment batch, rather than assuming that
light bounces or geometry influence are local. Source/occluder/circuit changes
conservatively invalidate lighting and rebuild all affected scene work.

Each batch key includes its complete authored props, actual support-height
transforms, tint, model variant, material metadata/content texture keys, light
control and caster classification. Unsupported normals/lightmap/sliver layouts
retain original streaming admission and chart order. Architecture reuse also
requires equal source geometry/grid/material dependencies, complete resolved
props and fixture-model presence. Trailing empty owner ranges alone are omitted
from the ephemeral lighting identity: missing and empty ranges both query as
None. The package writer still emits every original owner slot. Exact full
lighting records are compared separately during parity audits.

The previous build is a single optional owner, charged with its dependency keys
against a quarter of the CPU budget, capped at 512 MiB; oversized displayed
builds use weak ownership. Changing level discards the inactive entry. Architecture
uses Arc ownership and compiler edits use copy-on-write. Existing automatic shared
geometry/instancing, GPU upload bounds, full lighting and collision are preserved.
Cancellation cannot publish an incomplete cache entry. No production maps, assets,
shader behavior, compiler wire format, version or shipped menu package changed.

**Observed native latency.** Matched old C10 versus final editor binaries used
the same isolated Lantern edits and 1440x816 drawable. Worker wall time and
request-to-ready are separate seconds:

| Case | Worker preparation, before -> after (s) | Request-to-ready, before -> after (s) |
| --- | ---: | ---: |
| Move foliage prop | 14.872 -> 0.560 | 16.857 -> 2.539 |
| Rotate foliage prop | 14.862 -> 0.544 | 16.807 -> 2.558 |
| Room material | 15.013 -> 1.435 | 16.965 -> 3.457 |
| Cold Environment | 14.753 -> 14.957 | 16.675 -> 16.956 |
| Initial Inspect | 1.862 -> 1.899 | 8.936 -> 9.077 |

Move/rotate reuse 411/412 prop batches plus architecture; material changes reuse
412/412 batches and regenerate architecture. Scale/add/delete of nonoccluding
foliage prepare in 0.525-0.570 s and become ready in 2.482-2.533 s. Undo/redo and
12 rapid superseding edits restore the exact authored source and correct dirty
indication. Occluding road and structural room edits intentionally reuse no
Environment batches: approximately 15.00-15.10 s preparation and 17 s ready.
Lighting/circuit changes also retain whole-work fallback. Smaller performance-lab
prop/light/structural cases remain about 0.06-0.07 s preparation and 0.53-0.54 s
ready; its material edit falls from 86.53 to 51.29 ms preparation, 607.13 to
575.10 ms ready. This small single-pair campaign does not establish tiny timing
differences as regressions or wins.

The matched Lantern workflow peak working set changes from
1167.64 to 1019.71 MiB; peak private commit from
1815.53 to 1408.87 MiB. These are Windows process lifetime
peaks including renderer/driver allocations, not live heap or VRAM. Run duration
is 130.84 -> 47.67 s; total process CPU is 130.20 -> 36.78 s. Cached-edit upload
handlers remain around 4.35 ms p95; maxima fall from about 23.8-25.7 to 17.0-18.4 ms
in the three highlighted edits. The first cold Environment handler still peaks
at 45.67 ms versus 42.83 ms. Ordinary action handlers remain approximately
53.89 -> 53.42 ms p95 and 104.24 -> 104.07 ms maximum. Input stalls are not fixed.

**Timing limits and cold work.** Native editor latency above is a descriptive
single matched pair, with filesystem caches unspecified and focus snapshots
retained (often false). The editor currently hard-codes Fifo and exposes no
V-sync override. Therefore these ready timings do not establish the requested
uncapped editor performance acceptance, and no FPS/GPU throughput claim is made.
The independent CPU-only audits have no renderer, presentation pacing or GPU bake.
Cold Environment is 204 ms slower in preparation and 281 ms slower to ready in
this pair; initial Inspect is 38/141 ms slower. Cold/no-regression acceptance
remains open. Initial Inspect still has about 2.95 s of UI response wait and
9.08 s total ready latency despite 1.90 s worker preparation. This warrants the
already queued conditional cold-bootstrap investigation; this phase does not
attribute that gap to a specific pipeline or claim it fixed. Charger-limited GPU
state is a known condition; no further clock investigation or system changes occur.

**Correctness and required checks.** Final `editor-profile --preview-audit`
compares 21 Lantern and 21 small-scene cases against forced fresh builders,
including whole mesh, prop and full compiled lighting hashes: all 42 are exact.
It covers move/rotate/scale/add/delete, materials, light/circuit changes,
structural changes, metadata, Inspect/Environment switching and restoration.
These explicit developer audits take 466.72 s and 3.91 s; they are outside default
tests and CI, with no GPU or shipped-map bake.

Native cached-versus-new-process Environment rendering is pixel exact in the
981x539 viewport crop (0 differing channels); UI path/dirty text is excluded.
All 16 Full/Static circuit state images match the preserved runtime baseline
pixel for pixel at 1280x720 DX12. These fixed-delta correctness runs request V-sync
off and log actual Immediate presentation; they are not FPS benchmarks. Visual
inspection confirms rendered content rather than a blank equality. Native edits,
undo/redo, rapid supersession, close during preparation and fresh reopen pass.
World switching drops active model count 63 -> 17 -> 63, discards inactive reuse,
and returns to exact Lantern source with zero reused old-world batches.

Cold and cached real Build & Play pass on the small canonical fixture. Export
validates and passes require-current provenance; nine content-addressed core
mesh/lighting/collision/navigation/prop records match the prior compiled fixture
exactly across both variants. Shipped maps were not rebaked. One functional
script's final invisible ProjectSelect target failed after the edit checks;
the corrected visible Next sequence passes in a separate run. A circuit harness
trial omitted its bench flag, was stopped after checking the owned process, and
was rerun successfully in fresh evidence directories. These rejected trials are
retained and are not reported as product failures or successful measurements.

Final strict workspace/all-target/all-feature Clippy with -D warnings, locked
release workspace/all-feature build, formatting and all 316 Rust tests pass.
Two added tiny owner tests protect immutable batch/COW edits and exact dependency
fallback/inactive-owner release; they add essential cache regression coverage
with millisecond execution and no shipped-map/GPU dependency. The 24 targeted
Python checks and asset/texture/prop checks pass (629 assets, 389 placeable,
9 themes; existing catalog advisories retained). The broader historical
17 Python failures/errors were not rerun or fixed in this phase.

**Evidence and cleanup.** Sole-writer work starts from remote-matching
5e0724e04b8e4a2dc7e617eb0a15437821a0087a on 0.13.0-staging. Final source hashes
verify 484 Rust/shader files against the measured build. All 21 selected
EXE/DLL/PDB files are preserved and re-hash-checked outside target at
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-24/task-4/editor-preview-evidence/candidate8-bin`.
`final-validation.json`, `measured-outcomes.json`, parity files, native receipts,
captures and source/payload manifests are in that evidence root, indexed by
`local/windows-validation-0.13.0/editor-preview-incremental/evidence-index.json`.
The folder named candidate7-small-edits was launched with the final candidate8
binary; its identity SHA is checked explicitly. Historical evidence is untouched.

Cargo clean removes 7,811 files / 10.6 GiB logical. Disposable task-owned copies
remove 426139577 bytes. Large traces compress from
77196289 to 4251728 bytes, each decompressed SHA checked before removal.
Free space after cleanup is 82028793856 bytes. Preserved binaries then pass a
real editor/DLL/Environment smoke with target absent. Repository assets are still
used; this is not validation of the queued portable ZIP. Exact commit/remote and
final custody are recorded in ownership-handoff.json. Awake PID 53180 remains
active with bounded SYSTEM-only expiry 2026-10-25 16:32:30 UTC, handed to the
remaining queue without permanent power/security changes.

**Remaining combined acceptance.** Prior runtime Lantern readiness +77 ms,
Pool +8 ms and Pool peak private commit +1.2 MiB remain explicitly open, alongside
the campfire instancing tradeoff, Home/Pool favor/reward/restart, pressure and
final integrated FPS acceptance. Mac Metal/Linux native validation is still open;
finish Windows before switching. The retained auxiliary upload pipeline remains
two sheets; the denied four-sheet trial was never executed or retried.

The user explicitly approved migrating the actual runtime
`assets/shared/menu/scene.bin` before final Windows packaging. That next-stage
handoff is authorized but not performed here: baseline 17,025,419 bytes, 19 Deflate
entries, PLC1 revision 1, with 15,396,140 compressed lightmap bytes. Prefer supported
lossless Zstandard/PLC1 revision 2 packing without a rebake, verify decoded
lighting/geometry/scripts/events/provenance/native appearance and report actual
bytes/revisions. PPS1 revision 2 already shares attributes; revision 4 is cost
based. No menu size saving is claimed. Preserve separate shared assets. Then
complete combined acceptance and the requested portable Windows game/modes/demo
ZIP, validate clean extracted launch without Rust/Python/editor and exclude
private/developer files. Delivery is to the user; no friend contact, video,
public release, main merge, version bump or GitHub CI wait occurs.


## Bounded cold-bootstrap attribution and runtime menu storage (Windows, 2026-10-24)

**Result.** The actual tracked `assets/shared/menu/scene.bin` now uses the supported
lossless storage migration. It is **15,558,745 bytes**, down from **17,025,419**:
**1,466,674 bytes / 8.6146% saved**. Native appearance and representative events
remain exact. The cold-start investigation retains attribution and an explicit
uncapped editor test path; it retains **no speculative bootstrap optimization**
and does not claim that cold input stalls are fixed. Existing preview sharing,
RAM/instancing reductions, shader specialization, character pipeline preparation
and the two-sheet upload bound remain intact.

**Editor mode and timing.** `places-editor PROJECT --performance-test` requests
Immediate and removes the normal unconditional 16 ms loop sleep. Unsupported
Immediate surfaces fail with a named error; normal editor pacing and saved player
settings retain their prior semantics. `PLACES_LOAD_TRACE` enables buffered frame
and pump records with successful-presentation flags, actual mode, physical size,
focus and SDL display refresh. Startup and lazy pipeline stages are opt-in too.
This supersedes the preceding editor phase's missing test-mode path. Its historical
Fifo figures remain latency evidence, never uncapped FPS acceptance.

Two fresh-process Lantern workflows use **DX12, actual Immediate, 1440x816**,
no loop sleep, no fixed delta and no capture. SDL reports **165 Hz**. All four
editor controls/repeats have zero unsuccessful presentations. The small-scene
Immediate control has 3,621/3,633 ready pump intervals below the 6.061 ms refresh
interval (median **1.466 ms**), proving that this path is not capped at display
refresh. Focus is recorded and often false; this is path/latency validation,
not the remaining focus-controlled target-resolution FPS acceptance campaign.
Reported seconds are CPU preparation and request-to-ready, separately:

| Case | Preparation range (s), two Immediate runs | Request-to-ready range (s) |
| --- | ---: | ---: |
| Initial Inspect | 1.977-2.007 | 3.839-3.842 |
| Cold Environment | 14.845-14.944 | 16.057-16.140 |
| Warm foliage move | 0.563-0.568 | 1.807-1.834 |
| Warm foliage rotate | 0.556-0.560 | 1.816-2.026 |
| Room material | 1.464-1.480 | 2.934-3.093 |

Moves/rotations retain unchanged batch/architecture reuse and exact source-history
restoration; material edits retain the large previous savings. The normal Fifo
latency control prepares Environment in 14.749 s and readies it in 16.694 s;
move/rotate preparation is 0.540/0.541 s. Different pacing changes upload polling
opportunities, so the shorter Immediate ready latency is not a renderer throughput
win or an improvement to normal user startup. Filesystem/driver caches were not
flushed. These are cold **process/renderer** paths, not cold-disk measurements.

**Cold attribution and bounded decision.** The first normal native bootstrap
spends **615 ms** in adapter/driver selection, **96 ms** in device creation,
**16 ms** in instance setup and **18 ms** in project renderer resources. The two
Immediate Lantern starts spend **236-356 ms** selecting the adapter, **68-104 ms**
creating the device and **18-20 ms** in project resources. Pipeline creation costs
**2.253 s** in the normal control and **2.415-2.422 s** in the Immediate repeats;
world-surface/capture calls dominate. These API-call groups include WGSL validation,
DXC and driver PSO work, not a claim to isolate pure DXC CPU time from every driver
operation. Main-thread placement is project controlled; device/driver initialization
is separate from those later pipeline calls and from worker map preparation.

The longest editor pump gaps are **2.345 s** normal and **2.452-2.458 s** Immediate.
The small scene prepares Inspect in 207 ms but waits a further **2.075 s** for the UI
while initial pipeline creation costs **2.249 s**. This isolates a real initial
input-polling stall independently of Lantern's map size. Cold Environment's separate
worker cost remains **14.845-14.944 s**, with **41.8-42.4 ms** maximum upload handlers.
Ordinary scripted action handlers still peak around **101.5 ms**. Handler timings
exclude queue delay; pump gaps bound periods when new input cannot be polled.

The fresh runtime menu functional pair likewise records **2.273/2.138 s** of pipeline
creation and **2.308/2.168 s** maximum pump gaps before/after storage migration.
Variant decode stages total **617/430 ms**, with other preparation **48/47 ms**;
CPU delivery overlaps the blocked rendering thread, so its elapsed request span
must not all be called map work. Launch-to-world-commit is **4.105/3.874 s** in this
single fixed-step capture pair. It is descriptive attribution, not an accepted
startup speedup: cache state and compilation variation remain uncontrolled.

The pinned wgpu-hal 30.0.1 DX12 implementation has a unit `PipelineCache` and a
stub `create_pipeline_cache`; it offers no retained binary cache to enable here.
Existing asynchronous character pipeline work is not duplicated. Removing needed
pipeline variants or introducing a new threading/cache lifetime architecture would
expand the change and validation burden. No such speculative variant was run or
retained in this bounded pass. Input stalls and strict cold/no-regression acceptance
remain open. No clocks, driver, fan, charger, power-plan or quality changes were made.

**Actual menu transport and exactness.** `places-compile optimize-storage` validates
the preserved baseline, writes a distinct destination and never bakes. Balanced
Zstd 9 produces 16,296,723 bytes; explicit compact Zstd 19 produces the selected
15,558,745 bytes. Compact migration takes 11.65 s in this unisolated tooling run;
that is not a compiler-performance acceptance measurement. Both layouts are
cost-selected using the supported bounded policy. The actual runtime resource is
then replaced only from the validated selected file, with automatic baseline
restoration if native/parity validation fails.

| Stored property | Original runtime | Selected actual runtime |
| --- | ---: | ---: |
| Archive bytes | 17,025,419 | 15,558,745 |
| Decimal MB / MiB | 17.025419 / 16.236705 | 15.558745 / 14.837976 |
| Entries | 19 | 19 |
| Sum of uncompressed entry bytes | 61,272,098 | 45,314,957 |
| ZIP methods | Deflate 8 | Deflate 8 + Zstd 93 |
| Compact lightmap revision | PLC1 rev 1 | PLC1 rev 2 |
| Encoded lightmap bytes | 52,931,016 | 36,973,828 |
| Compressed lightmap bytes | 15,396,140 | 14,523,942 |
| Prop recipes | PPS1 rev 2 | PPS1 rev 2 |

The expanded six-channel half-bit atlas hash is identical. The full-precision
lighting/circuit record, meshes, collision, navigation, reflection records,
semantics/scripts/events, external dependencies and original build-input provenance
are exact. Both Full and Static/Off decoded prop records match. Revision 4 was not
forced over the smaller existing exact revision-2 attribute sharing. The expanded
runtime atlas is unchanged; archive saving is not a measured runtime RAM reduction.

Native matched **1280x720, DX12, actual Immediate**, fixed 1/30-second functional
runs use a fixed menu seed. **19 image pairs are pixel exact** (zero differing
channels). Parcel relay states 100-115 and blackout/roach states 500-506 complete;
all **26/17 respective traced state snapshots** match exactly, including door
states, circuit states, agent simulation and carry ownership. Captures visibly
show the original parcel relay, full blackout and restored scene with the roaches.
These capture/fixed-delta runs make no FPS claim.

A scratch junction asset overlay was refused by ordinary installed-asset admission
before renderer startup (exit 1). The owned PID had ended. The corrected runs use
the actual supported asset root; asset validation was never disabled. The failed
trial is retained as a harness receipt and excluded from successful comparisons.
An optional WMI refresh query also returned OS Access denied and was not retried;
the native SDL receipt supplies current display refresh without a remaining blocker.

Original SHA-256: `902134fbf796b4f2aee2807f37fc6b192ee83a23bde2283cad7043d85d2f5f15`.
Selected actual SHA-256: `bbb7cb766b5962d3c2f09b9cade186012500e4fe8213ca2ee0d71b665e3c660d`.
No newer package exists under `projects/menu`; the original solved runtime payload
is the migration source. **Baseline and selected package both remain stale for
current source/preparation inputs** (`source bytes changed`; `compiler preparation
code changed`). Their original source SHA and compiler fingerprint are preserved,
not relabelled current. This storage-only migration does not close source-currency
acceptance or authorize a replacement bake with different runtime semantics.

**Gates, evidence and cleanup.** Final formatting, strict workspace/all-target/
all-feature Clippy with `-D warnings`, locked release workspace/all-feature build,
all **316 Rust tests**, two focused Python ZIP lifetime/routing checks, and asset /
texture / prop checks pass. Actual installed menu typed validation has intact
dependencies and no warnings. No tests were added and no shipped map was baked.
The prior 42 preview parities, 16 circuit images and 24 Python checks remain dated
accepted evidence, not rerun claims; the known broader 17 Python failures remain.

This sole-writer phase starts at remote-matching
`a87d1a68cdae4fea3737a62ac84292ebb7248fce` on `0.13.0-staging`.
Evidence is in `C:/Users/Connor Dawkins/Documents/Codex/2026-10-24/task-5/cold-menu-evidence`,
indexed by `local/windows-validation-0.13.0/cold-menu/evidence-index.json`.
`editor-measurements.json`, `runtime-cold-attribution.json`, `menu-equivalence.json`,
native parity/event receipts and `final-validation.json` are the measured outcomes.
The 485 Rust/shader source hashes and all 11 selected EXE/DLL/PDB hashes are checked.
The preserved payload is `selected-bin` (93,054,200 bytes),
with source/payload manifests. Historical evidence and prior owners are untouched.

After every build/native task ended, Cargo clean removes **6,305 files / 7.6 GiB
logical**; actual free-space recovery is **7,658,115,072 bytes
(7.13 GiB)**. Checked task-owned disposable
copies/junctions recover another **47,554,560 bytes** actual.
Free space immediately after cleanup is **81,514,000,384 bytes**. No broad temp
or user-data sweep occurs. All 11 preserved file hashes are checked again, and
real editor Environment plus actual-menu game/DLL smoke pass with `target` absent.
They still use repository assets; this is not the queued clean-extracted portable
ZIP validation. Baseline menu evidence is retained. Awake PID **53180** remains in
queue custody through **2026-10-25 16:32:30 UTC**, with no persistent power changes.
Exact commit/remote/clean state and writer release are in the final ownership handoff.

**Remaining combined acceptance.** Lantern readiness +77 ms, Pool +8 ms and Pool
peak commit +1.2 MiB remain open, alongside mixed final FPS/instancing/campfire,
Home/Pool favor and reward/restart, pressure, and final integrated Windows checks.
Cold pipeline/input stalls and existing menu source currency are not claimed fixed.
Issue reconciliation and the authorized portable game/DLL/assets/modes/demo ZIP
with clean extraction/user delivery are next, serialized after this writer releases.
Mac Metal remains a later user-mediated switch after Windows; no Mac/Linux native
coverage is claimed. No main merge, release, version bump, CI wait, comparison video,
four-sheet retry, public upload or friend outreach occurs in this phase.


## Final combined Windows validation and portable package (2026-10-24)

This sole-writer phase starts at remote-matching
`d943bf6617fc4d7ac41812abe5670a0d7779cd9c` on `0.13.0-staging`.
No production Rust, shader, map source, visual asset or menu event changed.
All **485** Rust/shader file hashes match the accepted preceding phase. A fresh
locked release rebuild supplies the final player SHA-256
`a66d6ad4c266dfa161d3a7afa77ea461d068a5341cdb0d26b8044d48a4ec4fdb`; a rebuild is not claimed binary-identical to the earlier
player. The final staging commit adds the installation/reconciliation report;
exact local/remote identity is recorded in the local ownership handoff. The
observed native menu label is **0.12.0**, while `Cargo.toml` is already
**0.13.0** at this commit. No version change occurred in this phase; the staging
ZIP is not a public release tag.

### Aggregate and native acceptance

| Check | Final result |
| --- | --- |
| Format / strict Clippy / release | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo build --locked --release --workspace --all-features`: all exit 0. |
| Rust | `cargo test --workspace`: **316 passed**, zero failed/ignored. No new tests or automatic map bakes. |
| Assets | **629 assets / 389 placeable / 9 themes**, zero validator warnings; texture and prop checks pass with existing advisories retained. |
| Targeted Python | Compiler benchmark, lightmap harness, native regression-map and package ZIP modules: **24 passed**. |
| Broad Python | Not rerun in this phase. The accepted broader suite retains **8 failures + 9 errors = 17 baseline failures**, with 16 skips. No new broad-suite delta or full pass is asserted. |
| Fresh Home favor | Ordinary keyboard movement, real cushion pickup/carry, caretaker hand.R attachment, whole-assembly occlusion retirement and completion save pass on the rebuilt player. |
| Fresh Pool favor | Ordinary route, room door, actual duck pickup/release into water, upright flotation and completion save pass. Home then Pool produce the exact two completions in one real save. |
| All-favors reward / restart | Real reward-room entry, Keeper dialog/repeat and restart retain the exact eight receipt bytes. This reuses the earlier eight ordinary native completions, rekeyed only by filename for the installed path; only Home/Pool were freshly earned here. No completion flags were inserted. |
| Pressure / lifetime | CPU 1,024 MiB and GPU 768 MiB admission refusals preserve Home and the actual held cushion. These are policy-threshold tests, not physical-memory exhaustion or hardware calibration. |
| Final extracted Full lifetime | CPU preparation cancel, GPU upload cancel, Office rollback/reuse, Full Lantern recovery, menu unload, reload from menu and final return pass. Noncurrent retired ownership, final retired ownership and pending work all reach zero. |
| Package / play | All six project packages validate against staged assets. All **16** extracted levels reach Playing, render a native capture and accept ordinary movement input. Only exact authored Closing Time/Relay informational notices are allowed; no unexpected script diagnostics. Main menu opens Explore through normal keyboard input. |
| Rendering controls | Final Full Lantern, normal versus separate-instance submissions: two image pairs exact. Five-bind-group fallback: campfire exact, street differs by one 8-bit step in two color channels of one pixel. Four pairs total at 1280x720 High/Full; not four bit-exact pairs. |

Initial Home/Pool route misses and a wrong reward-dialog test expectation were
corrected in the external acceptance harness, with failed receipts retained.
The first 384 MiB GPU probe refused initial Home and was not a rollback test;
the 768 MiB case exercised the intended refusal with an already-held cushion.
The initial CPU action script exceeded the documented 60-second bound and did
not exercise product behavior until corrected. A capture-only camera confused
the spawn x/z/yaw syntax with xyz, and was corrected to the measured eye.
An alpha-only image-bounds default was caught by the independent nonzero-channel
count; final parity checks every RGBA channel. The two-channel one-step fallback
difference is retained, not hidden as an exact match. An initial Python stdout
decoder used the Windows code page for a Unicode path; explicit UTF-8 corrected
the harness before game playback. These are not hidden passes.

An **in-place Full Lantern reload** is refused by the unchanged default 2 GiB
GPU admission policy: displayed **1,250,487,316** + incoming **1,215,073,300**
charged bytes exceed **2,147,483,648**. The current scene remains usable and the
message instructs return to menu before retrying. It is not a successful
in-place reload. The first combined menu-input assertion after that modal also
failed; a second attempt selected Settings because the pause selection correctly
persists. Those receipts remain incomplete. The final ordinary-input script
respects that retained selection. The final dedicated Full sequence
uses the supported unload-to-menu/reload route and verifies ownership at both
menu checkpoints. No budget/sheet bound was raised to force a pass.

The exact staged EXE required interactive app approval. The first consent wait
timed out without launching; after Connor confirmed approval, process/window
checks established no prior instance and the one retry launched successfully.
The desktop screenshot service then timed out; this attempt is not a visual
pass. A normal targeted close succeeded. Native renderer captures, traces and
ordinary-input playback supply the recorded visual/gameplay evidence.

### Actual menu currency and retained optimizations

The shipped menu remains **15,558,745 bytes**, SHA-256
`bbb7cb766b5962d3c2f09b9cade186012500e4fe8213ca2ee0d71b665e3c660d`.
Its typed LevelDef is identical to a current preparation after normalizing only
the script's **252 CRLF** line endings to LF. Geometry, material/entity data and
all seven event definitions match. The stale developer provenance flag is
harmless metadata in this case, not evidence of stale menu content; no rebake
or fingerprint patch is justified. The earlier exact decoded records and 19
native image pairs remain accepted. The **8.61%** lossless saving is retained.

Cold pipeline/input stalls remain about **2.4 s**. No bootstrap optimization was
reopened. The accepted editor preview preparation of **0.560 s move, 0.544 s
rotate and 1.435 s material** remains; cold Environment preparation remains about
14.96 s. Accepted shared geometry/compatible instancing, bounded runtime uploads,
two-sheet mip work, handoff protection and shader specialization are preserved.
The historical Lantern **1,964 -> 865 MiB** settled-private and tree GPU-buffer
**494.66 -> 81.55 MiB** measurements retain their original workload/cohort; they
are not equated with a new Full-lighting load peak.

### Representative uncapped performance, separate from latency

Both binaries use the exact same completed Full package
`cd86cf2920b929e6ff411d9d0a59558c5acba7a656fa9e4924c565384188d5a3` and assets on native Windows DX12 / RTX 3070 Laptop.
Baseline player SHA is `caa80df645c3a994f701da5cc127fe5b78f353bbb75eda418e48c7a144b4584b`. These are final-cohort
comparisons, not a new before/after isolation of every historical optimization.
All runs confirm actual **Immediate**, VSync off, no frame limiter/fixed delta,
no readback and **2560x1440 physical drawable**, with native focus true.
Refresh is 165 Hz; timings vary away from refresh multiples, without a refresh
plateau. The initial Lantern pair overlapped asset auditing/staging and is
retained as exploratory warmup, excluded from the comparison below. The two
remaining interleaved Lantern pairs use medians; other cases are single pairs.

| View | Pairs | FPS before -> final | Loop p95 ms | GPU pass union mean ms | Request-to-ready seconds |
| --- | ---: | ---: | ---: | ---: | ---: |
| lantern/fixed | 2 | 46.59 -> 47.36 | 25.759 -> 26.069 | 20.897 -> 20.395 | 12.678 -> 12.928 |
| lantern/campfire | 2 | 71.21 -> 72.88 | 22.797 -> 22.459 | 12.071 -> 11.398 | 12.678 -> 12.928 |
| arcade/fixed | 1 | 89.44 -> 89.93 | 12.367 -> 13.154 | 10.622 -> 10.529 | 7.453 -> 7.373 |
| pool/fixed | 1 | 97.93 -> 97.64 | 18.747 -> 18.654 | 9.184 -> 9.179 | 7.565 -> 7.706 |
| home/fixed | 1 | 76.06 -> 76.92 | 29.052 -> 28.969 | 10.716 -> 10.571 | 3.951 -> 4.104 |

The small FPS differences and mixed tails/latency do not establish statistical
significance or universal improvement. Lantern's larger Full workload differs
from the historical Off-lighting cohort. Peak process commit (not settled RAM
or GPU memory) is roughly **2,468 -> 2,456 MiB** for the retained Lantern pair
medians, **1,862 -> 1,829 MiB** Arcade, **991.6 -> 991.8 MiB** Pool and
**818.7 -> 815.3 MiB** Home. Request-to-first-scene, CPU update/render, swap,
p95/p99, individual repeats and memory samples remain separate in the raw
receipts. The strict zero-regression criterion is **not passed**: earlier
Lantern readiness +77 ms, Pool +8 ms / +1.2 MiB and mixed campfire evidence remain
dated residuals, and this final sample also has slower readiness in three views.
No speculative optimization, quality reduction or four-sheet trial is added.

### Portable Windows distribution

The inspected diagnostic collection had no common lighting variant across all
nine maps. Normal compiler exports added missing **Full** variants for Office,
Lantern, Winter and Aero; the five existing Full members are retained. Prepared
semantics match the original maps after script EOL normalization. Source
snapshots, all compiler records, dependency identities and provenance survive;
no stale fingerprint was made current by editing metadata. Full transport ran
only for the missing distribution variants, outside tests/CI, not as a new
compiler performance campaign. The composed archive validates with zero
warnings; lossless storage parity is exact for every member. Collection bytes
are **389,366,309 -> 376,847,263** through lossless storage. Comparing the new
Full collection with the smaller mixed-quality diagnostic input is not a
matched-content size comparison.

`Places-0.13.0-Windows.zip` contains one game EXE, six app-local DLLs (SDL3,
DXC/DXIL and the three required Microsoft VC runtime libraries), runtime assets,
licenses and six projects: Quiet Collection (9), Beam (1), Closing Time (1),
Relay Annex (3), One Guest Too Many (1), Shape of an Exit (1). **Lightmaps Full**
is required across the collection; the package does not contain a complete
Static variant set. This limitation is explicit in the README/installation
instructions. No game mode, level, texture quality or approved event was removed.
Explore has eight visible entries because all three Relay Annex sections are
exposed by its existing project metadata; the collection exposes only Hub.

The ordinary Deflate ZIP has **978 files**, unpacked
logical size **714,834,103 bytes**, archive size
**658,523,027 bytes**, SHA-256 `9e39f4be4d36c4f446054f19cd1a4e65e8204316ec89b24c3e30239d21b5c97c`.
Every ZIP CRC and every extracted file hash matches. Extraction is outside the
repository in a path containing spaces and a non-ASCII character. `--paths`
resolves sibling assets and isolated normal user folders from an unrelated
working directory; child PATH contains only Windows system directories. Native
module enumeration confirms all six runtime DLLs load from the extracted
folder. Rust, Python, compiler and editor are absent from the payload and child
PATH; the external QA harness is not a runtime dependency. All payload hashes
remain unchanged after play. Source artwork/masters, editor assets/tools,
symbols, repository files, logs, scratch evidence and QA saves are excluded.

Library attachment delivery was subsequently cancelled by Connor. The earlier
preparation failure is retained in the local historical handoff; no upload
retry or delivery remains pending. No friend was contacted and no public
release/upload was created.

### Reconciliation, cleanup and remaining gates

Issues #25 and #26 were already closed. #17, #18, #21, #24 and #27-#31 receive
precise completed-Windows/remaining-scope reconciliation and stay open where
their compound criteria remain incomplete. #24's ordinary sheet ghost and six
review-only modular rail/fence paths remain pending. #31's requested video/
montage criterion is removed following Connor's cancellation. #14/#15 retain
their 0.14 scope untouched. No extra architecture or asset adoption was added
to manufacture umbrella-issue closure.

After native/build users of `target` end, final binaries, compiler/repacker,
package and minimal useful evidence are preserved outside it and rehashed.
With `target` absent, a final preserved/extracted Home launch, native capture
and ordinary movement pass again using Windows-only PATH and sibling assets.
The ZIP, extracted payload and preserved tool hashes match after cleanup.
Cargo cleanup records **Removed 6224 files, 6.9GiB total**, with actual
Cargo free-space change **6,968,594,432 bytes**.
Checked task-owned redundant staging/helper removal and lossless evidence
compression change free space by another **1,827,991,552 bytes**, for **8,796,585,984 bytes** total observed
recovery. Detailed paths and per-file hashes are in `cleanup.json`. Existing
recovery evidence and unrelated work remain. No comparison recordings, power
changes, clock/charger/driver investigation or denied four-sheet retry occurs.

Raw receipts are under
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-24/task-6/final-windows-evidence`;
the local `windows-validation-0.13.0/final/ownership-handoff.json` records final
commit/remote identity, payload/delivery hashes, cleanup and explicit writer
release. Native clean-OS Windows/installer/download trust, long-session and
hardware-budget calibration, unresolved strict resource/tail criteria, final
combined **Mac/Metal**, and **Linux** remain open. This Windows staging push
does not wait for CI, merge main, publish a release, bump the version or claim
the final cross-platform version gate. The parent coordinates the later
user-mediated Mac switch after Windows custody is safely released.


## Combined-map entry audit (2026-10-24)

The shareable ZIP contains **six distinct playable projects**, with **16 internal
compiled sections**. All six archive hashes, all 16 stable section IDs and all
16 semantics-record identities are distinct. No obsolete standalone exports or
duplicate map archives were found; **zero map/archive files were deleted**.
The live-menu scene is outside that count, and the reward room is part of Hub.

Relay Annex's three Explore cards were its linked Intake, Tower and Discharge
sections. Its source manifest now sets the existing `menu_entry_only: true`
policy, giving one Relay Annex entry on future normal exports. All three member
sources, scripts, thumbnails and transition declarations remain. Normal metadata
export reuses every prepared world in 6.04 seconds: only `build-inputs.json` and
`manifest.json` change for each member; gameplay, geometry, lightmaps, collision
and other compiled records are byte-exact. The proof bundle validates. Native
Explore shows **six** project entries, and the existing project loader reaches
Intake -> Tower -> Discharge -> Intake with internal rows hidden. This is an
explicit loader diagnostic, not a fresh objective/airlock playthrough; the
authored transition scripts and conditions remain byte-exact.

The original validated ZIP is preserved unchanged at **658,523,027 bytes**, SHA
`9e39f4be4d36c4f446054f19cd1a4e65e8204316ec89b24c3e30239d21b5c97c`.
It still has the previous eight-card menu metadata. The parent queued a larger
Beam campaign next and requested no new full distribution here. The next
assembly must use the refreshed Relay component (or re-export its now-correct
source) so only one Annex card appears. No Library retry, broad build, new art
or campaign expansion runs in this audit. Rust/shader sources are unchanged,
so the previous strict gates are retained rather than reported as rerun.

Evidence and the refreshed Relay component are under
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-24/task-6/redundancy-audit/`.
The repository writer is released after this bounded metadata/report commit;
the queued four-section Beam expansion is a separate task.

## Beam four-section campaign (2026-10-25)

Beam remains **one playable map**, now containing Home -> Pool -> Office ->
Arcade. Stable entry `beam` links `beam_pool`, `beam_office` and `beam_arcade`;
`menu_entry_only: true` hides the internal rows. The combined distribution
still has six playable projects, now 19 internal sections. Relay Annex keeps
its three linked sections and corrected single entry. Zero unique maps were
deleted. The old ZIP remains unchanged pending the later queued work.

The maintained two-storey Home gained a furnished 128-square-metre west wing
with kitchen, dining and utility spaces, plus an upstairs bedroom. Pool is
26 x 28 m, with changing rooms, wet deck, a sunken basin, a substantial sloped
bank and service rooms. Office is 24 x 28 m, with nine workstations, filing and
break rooms, reception and a meeting-room circulation loop. Arcade is 28 x 30 m,
with 18 cabinets, two pinballs, party seating, bowling and a prize foyer.
Together the four sections have **247 props, 25 fixtures and 12 physical
collectibles**, over 2,626 square metres of gross floor area including Home's
upper section. All visual assets are existing supported repository assets.

| Section | Physical objects | Return furniture |
| --- | --- | --- |
| Home | Kitchen mug, upstairs books, cushion | Entrance TV cabinet |
| Pool | Duck, blue towel, detergent | Plastic service table, service bench, empty rack's top shelf |
| Office | Records, mug, supply box | Filing cabinet, break-room table, reception counter |
| Arcade | Plush cat, star gift, blue bowling ball | Round prize table, prize counter, empty foyer ball rack |

Exact-instance receivers count only released objects. Picking one up or moving
it away clears its credit and relocks the exit. F1 names the next item and its
return furniture. Entrances are refuges. Forward doors advance, back doors
start a fresh previous chapter, the EXIT sign and death restart the current
chapter, and a menu launch starts Home. Final Enter replays Arcade. Inventory
does not transfer and there is no persistent chapter checkpoint. Arcade adds
a two-second uninterrupted flashlight check outside its foyer, within eight
metres of the creature, after the three returns.

The creature now has **no visible body**. Existing per-entity `shadow_only`
keeps its posed geometry in the actual handheld-flashlight shadow pass while
hiding colour, reflection and emissive geometry. Its shadow projects onto
ordinary floors, walls and props. Ceiling, ambient and baked light cannot
reveal it. Existing direct flashlight observation supplies cone, range and
occlusion eligibility and the hold/release behavior; no renderer change or
additional per-frame lighting query was added. Native snapshots distinguish
`mesh_visible: false` from `flashlight_caster: true`, including lit captures.

The base `293f8de` visitor and the original task-6 portable Beam's recovered
source both omit `shadow_only` and use reveal-only light response on the black
mannequin. The expansion copied that setting; this task did not remove an
existing shadow-only flag or implement a black-silhouette renderer. The
correction enables the existing projection-only mechanism on all four Beam
visitors. The available source/package history does not establish an earlier
version matching the user's remembered appearance.

Pursuit is 12% faster than the preceding authored rules: starting speeds are
0.728 / 0.8064 / 0.896 / 0.9856 m/s, plus 0.0672 per returned item, for maxima
0.9296 / 1.008 / 1.0976 / 1.1872 m/s. Faint attached spatial footsteps use the
existing `impact_soft` cue, emitter gain 0.22 and cue gain 0.36, before distance
attenuation. Actual travel accumulates a step every 0.55 m with a 0.35-second
minimum interval and a 12-metre playback gate. Holding or stationary motion
stops the step source. The normal master/effects controls and world/unload
lifecycle remain authoritative; no duplicate audio system or asset was added.

Introductions, item hints, objectives, reset/failure/completion and transition
text were rewritten in concrete English. Examples: "Only unheld items count;
removal clears their credit" became "Put the item down to return it. Picking
it up undoes the return"; "F reveals and holds it" became "F: flashlight
on/off. Keep the shadow in the beam to stop it"; the final line is "You
escaped. Press Enter to play Arcade again." Native checking caught an
overlong introduction after the shadow wording change. All script literals
are now within the guidance API's 72-byte line limit, and actual startup and
hint display are checked separately from syntax.

Geometry, furniture, fixtures, project membership/transitions and script
sheets were authored through the real Places editor. Reviewed source metadata
attached physical shapes, exact-item receivers and component bindings without
regenerating geometry. The historical `beam_editor.py` was not run. Save/reopen,
script checks and Safety reviews are retained. Playtests led to Home's chair
and doorway/endcap clearance fixes, a two-metre Office connector, wider Pool
service landings and a lengthwise towel bench. The detergent destination is
the accessible top shelf, while its source is a low shelf reached with C.
Bounded model overlap review reached its advertised spatial/pair budget, so
it is not an exhaustive triangle-clearance proof. Intentional support contacts
and the maintained ramp require context; no blanket placement ignores were
added. Bespoke patrol/last-seen/search pacing, refuge/catch and distance-step
logic are documented for the later shared-AI migration; existing native
navigation and light-response primitives still own movement and illumination.

Normal keyboard/mouse routes complete all four chapters and actual forward
handoffs, without pose/item teleport, activation, navigation-goal or forced
level-load actions. Home verifies 3 -> 2 -> 3 credit with relock/unlock;
Pool carries the duck through the basin bank and retains all three furniture
returns; Office reaches Arcade; Arcade performs the final beam check, wins
and replays with fresh deliveries. Three reverse doors reach fresh previous
sections. Death, EXIT-sign reset, F1 pages and safe spawns pass. A Pool route
that looked away was caught; the accepted route uses the flashlight while
passing the approaching creature. Counter/shelf carry aims were adjusted in
normal input and repeated; failed calibration evidence is retained.

Lit and flashlight-off screenshots cover the campaign with Full reflections.
Near hold/off/reacquisition captures cover Home, Pool and Office; Arcade has
a lit encounter and an off/range case.
The body mesh stays hidden while flashlight shadows appear on receiving
surfaces. Ceiling fixtures remain on with flashlight-off intensity/opacity
zero. Beam hold is stationary; darkness resumes actual pursuit. Additional
normal-input checks cover leaving the cone, a blocking wall, and a greater
than 24-metre range case which is also occluded (not an isolated range proof).
Existing renderer source independently enforces that range cutoff. Audio
checks record moving attached source positions, one step voice at a time,
silence while held, nonzero audible PCM and exactly zero PCM at master zero.
Those are native output/trace checks, not a claimed human listening review.

Sequential DX12 measurements on the RTX 3070 Laptop GPU use High, Full
lightmaps/reflections, VSync OFF, no frame limiter and actual **Immediate**
presentation at **1280 x 720 physical pixels**. The 165-Hz display does not
cap the measured means. Each sample is a 15-second stationary entrance view
with real gameplay delta and no framebuffer readback or compilation during
timing; these are neither whole-campaign worst-case results nor a claimed
before/after engine improvement.

| Section | Mean FPS | p95 loop ms | p99 loop ms |
| --- | ---: | ---: | ---: |
| Home | 305.43 | 6.108 | 6.968 |
| Pool | 260.35 | 6.477 | 6.917 |
| Office | 304.98 | 6.285 | 7.239 |
| Arcade | 244.97 | 6.348 | 7.094 |

The final normal full-project export is **56,528,928 bytes**, SHA-256
`c8dcf901ff6060a3d9086f436df76d8d15ffd012ff1d9e6bf403c68b1a89b56e`,
at `C:/Users/Connor Dawkins/Documents/Codex/2026-10-24/task-7/deliverables/beam-campaign.placesmap`.
Native Build+Play completed in 38.94 seconds after cached world reuse. The
preceding shadow-only world bake took 828.42 seconds. Project check, inspect,
validate, all four `--require-current` checks and recovery pass; all nine
embedded editable files are byte-exact to canonical sources. Both Static and
Full variants are present. Acceptance uses High / Lightmaps Full / Full
reflections. Direct asset closure checks found 119 files / 141 catalog
references identical to the prior portable extraction; final assembled-root
validation remains the deferred packaging task.

Format, strict workspace/all-target/all-feature Clippy, locked all-feature
release, **316 Rust tests**, and **629 assets / 389 placeable / nine themes /
zero warnings** pass. Texture and prop checks pass with existing advisories
(140 texture entries / 100 existing texture warnings). No tests were added.
Rust, shaders and global assets are unchanged throughout this campaign work;
recent instancing, lighting bounds, memory savings and strict two-sheet upload
bound are retained. The broader Python baseline's 17 failures/errors and 16
skips were not rerun and are not presented as a new all-Python pass.

Final binaries and six DLLs were preserved outside `target` and their ten
hashes matched the validated originals. The final component and all unique
recovery/source data remain. After native/build users ended, the verified
repository target path was cleaned with Cargo: Removed 6224 files, 6.9GiB total. Checked task-owned
duplicate imported archives were removed only after an exact preserved copy
was verified; disposable mixed-output audio captures were removed while the
two final isolated audio captures remain. Cargo's observed free-space change
was **6,977,597,440 bytes**; total observed recovery including
those disposable copies was **12,051,505,152 bytes**. No unrelated
files, unique source or recovery component was deleted.

Evidence is under `C:/Users/Connor Dawkins/Documents/Codex/2026-10-24/task-7`.
The ownership handoff records the exact staging commit/remote and writer
release. No force push, main merge, public release, version bump, CI wait,
upload, video, Windows security/power change or clock/charger investigation
was performed. The optional bounded awake helper was blocked by execution
policy before running. One retry under Connor's later explicit awake request
also exited 1 before execution; no policy was bypassed and no awake request
is active. An approved awake utility/helper is still needed for that request.
The existing roughly 2.4-second cold startup remains a known prior result.

Final portable ZIP assembly is deferred until the parent-owned release rough
edges, shared-AI work and new Backrooms map are complete. The original
658,523,027-byte ZIP is preserved, predates the Relay fix and must not be
represented as this new campaign. Later assembly must include the refreshed
Beam and Relay components, all existing modes/maps and runtime dependencies,
verify combined Explore entries from the final project set (six before the
queued new map), and pass extraction/runtime checks without Rust/Python. Attachment delivery was cancelled: no Library work is pending.
Mac/Metal awaits explicit user coordination; Linux, final version CI and other
previously documented release criteria remain open. The next general pass
also owns developer-console gating of ordinary prop/physics diagnostic text;
real blocking notices and logs must remain available.

Known separate visual issue: arcade machine screens can remain fullbright
when off, and machine light output needs review. Connor explicitly moved
that work to a subsequent task; it was not investigated or fixed here.


## Release rough edges and quiet ambience - Windows, 2026-10-25

This sole-writer pass began on clean `0.13.0-staging`, with HEAD and the actual
remote both exactly `eaa0be183482649d67984b99071ef3314c8a6891`. Both required
ownership handoffs released custody. Repository `AGENTS.md`, the relevant
authoring/audio contracts and prior release evidence were read; this checkout
has no `.agents/skills` directory. No map/project source, Beam rule, renderer
shader, Cargo version or lockfile changed.

**Version and bounded loading decision.** The current UI already receives
`env!("CARGO_PKG_VERSION")` through `APP_VERSION`; introducing another constant
or bumping Cargo would be wrong. The freshly rebuilt native player displays
**v0.13.0**, including a second successful menu launch after Cargo cleanup.
`native/menu-version/version.png` and the post-clean capture retain the actual
visible label. The deferred old portable ZIP was not rebuilt or replaced.

Cold pipeline/input stalls remain open. Current native runs record approximately
**2.166-2.361 s** in initial renderer-pipeline API groups and **2.197-2.394 s**
maximum cold event-pump gaps; the preserved old-player timing controls reach
2.395 s. Full values, including instrumented launch-to-ready spans, are in
`cold-attribution.json`. These groups include project WGSL validation, DXC and
driver PSO work, not isolated driver CPU time. The pinned DX12 pipeline-cache
implementation still supplies no retained binary cache to enable. The bounded
review retained no speculative bootstrap/threading/variant change and claims
no cold-start improvement. Shared geometry, compatible instancing, immutable
warm editor preview/cache ownership and the two-sheet mip-upload bound remain
untouched. No four-sheet experiment, driver/clock/charger/fan or power-setting
work was performed.

**Safe admission recovery.** The default **2,147,483,648-byte** map GPU admission
budget remains unchanged. Current Full Lantern attempts charge
**1,215,073,300 displayed + 1,215,073,300 incoming bytes** and are refused before
the live world changes. This is a conservative admission charge, not measured
physical GPU memory. The renderer now returns a typed `MapAdmissionError`;
the player offers unload/retry only when the incoming map fits by itself.
An individually oversized map cannot receive a futile unload offer.

The visible prompt explains the two-world overlap and explicitly warns that
unsaved in-map progress restarts. Separate drawn buttons and their actual hit
regions agree: **Enter / unload + retry**, or **Esc / keep map**. Declining keeps
the actual Lantern camera and physical body count. Accepting unloads the current
renderer, simulation, items, script presentation, audio and displayed lease
before starting the exact retained source identity. No intermediate background
world is inserted, and no second prepared bundle is held for the choice.
The explicit fresh attempt remains cancellable: Escape reaches a usable empty
Explore screen with no renderer level, physics bodies, audio controls, pending
commit or displayed prepared bytes. A subsequent retry loads Full Lantern
successfully. No budget increase, quality rewrite or silent unload was used.
`native/reload-{keep,retry,cancel}/` contains captures and ownership snapshots.

**Developer-only prop modals.** `UiState::developer_diagnostic` always preserves
logging and gates the player-facing modal on `settings.developer_console`,
independently of whether the console is currently open. Exact converted paths:

- `apply_ragdoll`: refused start, safe carried-item release rollback, and reset
  of an actor without an active ragdoll (`app/ragdoll_runtime.rs`).
- Failed miniature-pin substitution (`apply_world_commands` in `app/mod.rs`).
- Refused NPC carried-item restoration (`release_carried_item_now` in
  `app/carry_runtime.rs`); attachment/rollback ownership remains intact.

An enabled diagnostic owns normal keyboard navigation and pauses gameplay while
its modal is visible. Native bounded fault injection calls the real missing-actor
ragdoll refusal with the setting disabled and enabled, checks hidden/visible
status, dismisses it normally and verifies both log messages remain. The
disabled no-active-ragdoll path stays quiet too. This is a deliberate diagnostic
fixture, not a claim to reproduce a naturally occurring ragdoll failure.
The miniature/carry branches share the tested presentation seam; their safety
checks and rollback code were not bypassed.

Other examined pathways were already log-only: `update_physical_items` recovery
events (including suspension), activation/reset-group refusals, and entity
`SetLocked`/not-a-door warnings. They remain available in logs/traces rather than
being globally swallowed. Objectives, authored guidance and actual load,
settings-save, asset and startup failure notices remain unconditional.

**Quiet shared ambience.** Three committed original mono PCM16/44.1-kHz WAVs
were authored offline under the repository MIT license, with recipes, hashes and
provenance: `fluorescent_buzz/01.wav` (7.5 s), `pool_lapping/01.wav` (23.5 s) and
`empty_room/01.wav` (29.5 s). The generator is an explicit authoring tool,
`tools/audio/original.py`; runtime only loads the files. The pool clip has rounded
lapping/wash and sparse small splash envelopes. The room clip uses sparse soft
upper partials, without abrupt stingers, dissonant rises or a subbass drone.
Subjective warmth/melancholy is an artistic intent, not a claimed listening pass.

The existing mixer owns at most **three additional spatial loops per map**:
one nearest powered fluorescent/UV ballast (0.5-6.5 m), one nearest rectangular
or circular water-surface point (1.5-11 m), and the restrained room tone
(4-18 m). Dense fixture/pool arrays cannot stack a voice per placement.
Positions and source envelopes blend over about 0.35 s, over the existing smooth
distance attenuation/tweens. Initial playback now applies authored attenuation
overrides immediately, rather than waiting for the next mixer update. The per-frame path visits cached placements and
fixed controls, without constructing vectors or strings or restarting loops.
Live fixture toggles use the same power handoff as rendering; world reset and
map unload clear ownership. Application pause, master zero and the ambience
slider remain authoritative. Catalog HVAC gain is **0.18 -> 0.065** (about
8.85 dB quieter). Ordinary HVAC/bird/water catalog defaults stay non-positional;
the new sources explicitly select spatial playback.

Native power-off retires the one buzz voice; power-on restores one. A valid dry
room retires both distant buzz and pool lapping. Three actual `restart_sign` /
`api.reset()` cycles restore exactly three expected beds each time, with unchanged
decoded memory and no queued voices. Map handoff retires the old water sources.
All native shutdowns end with zero controls and pending starts. Audible-mode PCM
has peak **0.079486** / RMS **0.009922** at the deliberately amplified test settings
(master/ambience 1, effects 0); master-zero PCM is exactly zero. Default
master/ambience gains are lower. The full library is **41.40 MiB decoded**, within
the unchanged 64 MiB cap, including **20.36 MiB** for the three new clips.
The offline audit passes **28 cues / 66 WAVs / 10,855,264 encoded bytes**.
Windows path normalization in that audit was corrected so manifest paths compare
as POSIX relative names rather than backslashes.

Steady native three-voice mixer-process spans average **44.55 / 45.68 us per
10-ms block** in Office/Pool, about **0.446 / 0.457%** of nominal audio time.
These are opt-in wall-time spans, subject to scheduler preemption; they are not
hardware CPU utilization. No callback file I/O or per-frame diagnostic allocation
was added. Voice errors, rejected requests and pending starts are zero in these
cases. The optional fixed developer PCM tap discarded 36,224 / 42,944 frames
before the first audio checkpoints during cold setup; later sampled counters
stay flat. This tap loss is reported separately from mixer/device errors, and
the captures are not presented as gapless wall-clock recordings.

The actual output stream, PCM, source positions and lifecycle were validated.
**Human/subjective listening remains open:** no listener or audio-perception tool
was available. The audible local PCM remains for review. One early "far" pose
left the valid floor and recovered to spawn; it is retained and explicitly
excluded as distance proof. `pool-distance-focused` supplies the valid dry-room
result instead. This bounded pass is not the later full audio leak/performance
audit and adds no occlusion, HRTF, material acoustics or reverb system.

**Focused throughput check.** Sequential High / Full lightmaps / Full reflections
runs use actual DX12 **Immediate**, **1280 x 720 physical drawable**, VSync OFF,
no frame limiter, and a 165-Hz display. Means exceed refresh comfortably, so
display pacing does not cap these samples. Each is one 15-second stationary
entrance view, flashlight off, with no capture readback or active compiler.
Both binaries use the same current audio catalog/assets. These views differ
from the earlier flashlight-on Beam acceptance and must not be equated with
its 305/260/305/245 samples. No general performance gain is claimed.

| Scene | Mean FPS before -> after | p95 loop ms | p99 loop ms |
| --- | ---: | ---: | ---: |
| Office | 389.37 -> 384.73 | 4.672 -> 4.734 | 4.959 -> 5.272 |
| Pool | 305.20 -> 309.95 | 5.891 -> 5.895 | 6.433 -> 6.560 |

Small mixed single-pair changes are consistent with ordinary run variation and
the added sound work. Beam's four linked sections, 247 props, 12 collectibles,
single Explore entry, literal invisible-body flashlight-cast shadow, pursuit
and footsteps are unchanged. Its accepted export remains **56,528,928 bytes**,
SHA-256 `c8dcf901ff6060a3d9086f436df76d8d15ffd012ff1d9e6bf403c68b1a89b56e`.

**Checks, evidence and cleanup.** Final format, strict workspace/all-targets/
all-features Clippy, locked all-features workspace release, **319 Rust tests
(zero failed/ignored; about one second execution)**, asset, texture, prop and
audio checks pass. The three new Rust regressions use tiny deterministic
fixtures: unload eligibility boundaries, enabled/disabled diagnostic presentation
and explicit modal hit ownership. No full map baking is part of tests or CI.
Assets remain **629 / 389 placeable / nine themes / zero warnings**. Texture
checking retains the known **140 entries / 100 advisories**; prop checks retain
existing art advisories. The broader Python baseline's **17 failures/errors and
16 skips** was not rerun or relabeled as a pass.

The final local audit has **58 passing native acceptance checks**, including
the preserved post-clean player launch. Evidence, settings, actions, native
captures/traces, output PCM, timing samples, quality logs and all ten preserved
binary/DLL hashes are under
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-25/task/`.
The current player SHA-256 is `4e760ff10f89024faf53737f263b1ceea843114f2aeb355d27823a267d69e774`.
`native-acceptance.json`, `fps-results.json`, `audio-steady-cost.json`,
`quality-final/results.json` and `quality-final/preserved-binaries.json` are the
compact receipts. `selected-bin/` contains the current four executables and six
runtime DLLs; this is not a newly assembled portable distribution.

All ten saved hashes were verified before cleanup, and the tested native player
matches the preserved player byte-for-byte. No build/native users remained.
The resolved non-junction repository target was cleaned with Cargo: **7,439 files,
10.1 GiB reported removed**, target absent; observed C-drive free-space change
was **8,674,185,216 bytes**. `cleanup.json` and `cargo-clean.log` retain both figures.
A subsequent direct preserved-player launch rendered v0.13.0 and shut down
normally without target. Verified duplicate imported packages and the temporary
native binary copy (**475,287,127 file bytes**) were removed only after their
canonical preserved originals matched. Muted PCM was losslessly compressed with
its decoded SHA-256 verified; audible PCM, unique sources, recovery data and
native evidence remain. See `duplicate-cleanup.json`.

The prior **658,523,027-byte** portable ZIP still has SHA-256
`9e39f4be4d36c4f446054f19cd1a4e65e8204316ec89b24c3e30239d21b5c97c`.
No duplicate ZIP, video or Library upload was produced. Final assembly must
include the new global audio catalog/WAVs and current binaries after the remaining
queue. Arcade screen/off-state and realistic machine lighting, compiler
optimization, water visuals/performance, advanced acoustics/full audio audit,
shared AI, editor work and Backrooms remain separate tasks. Mac/Metal awaits
explicit coordination; Linux and the final release CI gate remain open.

Connor's manual plugged-in sleep setting is the latest instruction. No power,
security or update setting was changed, restored or investigated, and no awake
helper was started. Routine staging commit/push is authorized; exact final remote
and writer release are recorded in the external ownership handoff. No force
push, main merge, public release or version bump is implied.

## Compiler follow-up: exact working buffers and independent reuse - Windows, 2026-10-25

This completes the three newly authorized compiler optimizations on
`0.13.0-staging`, starting from clean, remote-verified
`a98abc5ec3c74f758f702f0196605d8f76759769`. It does not complete the remaining
0.13.0 queue. Root `AGENTS.md`, the relevant canonical release/format guidance
and current implementation were checked. This checkout has no checkout skills.
No repository source map, visual/audio asset, quality setting, shader, dependency/version,
solver equation, sample count, bounce count or memory budget changed.

**Previous-bounce buffers.** The single and shared base/first-circuit paths now
retain only exact surface-light RGB in the previous-order vectors. The full
15-f32 direct, gained and total directional accumulators remain, including their
finite checks and original arithmetic order. Existing geometric stencils, BVH,
emitter filtering, scratch reuse and parallel work division are unchanged.
An RGB previous value is 12 bytes versus 60: **48*N theoretical bytes saved per
layer**, or 96*N for the shared pair. This is an allocation calculation, not a
prediction of whole-process resident-memory savings.

Six clean forced Full bakes ran sequentially with 12 workers and no concurrent
build, other bake, native game or decoder campaign. Home/Pool used control then
candidate; Beach reversed that order. File caches were not flushed and host
thermal/background variation was not controlled. These are **one pair per map**,
not a statistical speedup claim. Windows process counters were sampled every
50 ms; CPU includes all process threads. Peak working set and PeakPagefileUsage
(commit charge) are distinct; sampled current-private peaks remain separately
in the raw receipts. No GPU memory is added to these figures.

| Case | Wall seconds before -> after | CPU seconds | Peak working set MiB | Peak commit MiB |
| --- | ---: | ---: | ---: | ---: |
| Home Full | 84.333 -> 85.007 | 952.328 -> 960.422 | 546.9 -> 496.2 | 641.5 -> 570.4 |
| Pool Full | 220.220 -> 222.717 | 2,373.734 -> 2,407.062 | 844.0 -> 762.3 | 1,011.5 -> 943.9 |
| Beach Full | 277.377 -> 278.521 | 2,954.391 -> 2,966.562 | 2,285.5 -> 2,279.5 | 2,922.5 -> 2,604.7 |

Home/Pool working-set peaks fall approximately **9.3% / 9.7%**. Beach's working
set is effectively flat, while peak commit falls approximately **10.9%**.
All three candidate wall times are within about 1.2% of their controls, with no
whole-bake speed gain established. Initial Pool/Beach samples overlapped an
early Clippy build and are explicitly excluded from these comparisons. The
uncontended controls were rebuilt; every repeated control archive is byte-exact.

The preserved bounded Rust oracle compares every decoded lightmap half plane,
reconstructed prepared-prop/Static-off stream, mesh, lighting/probe/collision/
navigation record, semantics and dependency. **Home, Pool and Beach are exact**;
receiver, direct-ray, logical bounce and geometric-work counts are unchanged.
Only compiler-code/tool identity, compiler/lighting fingerprints and the declared
provenance-record digest are normalized. Other provenance fields, record names,
roles and lengths remain compared. These ordinary outputs are selected-level
build caches rather than complete distributable environment collections.
The retained standalone Home `validate` refusal is the expected complete-project
context guard. These linked caches pass bounded record reads and current-input
checks; the complete two-member export below receives full typed/native validation.

| Ordinary Full package | Before bytes | After bytes | Decoded lighting |
| --- | ---: | ---: | --- |
| Home | 11,096,833 | 11,096,825 | exact |
| Pool | 17,980,200 | 17,980,195 | exact |
| Beach | 17,808,563 | 17,808,561 | exact |

**Independent qualities.** The v2 common preparation key excludes requested
quality membership/order, while retaining actual prepared source, geometry/
solver/record revisions, assets, catalog closure, preparation code and capture
mode. Every declared old record is still checked; only requested existing
qualities are retained, and missing qualities are built. Navigation, semantics,
atlas keys and complete current provenance are refreshed. Results follow caller
order and cache decisions identify each quality. Old stage keys miss
conservatively once; a prior producer is not retrospectively blessed.

On the existing material-transport fixture, Full -> Static+Full takes
**15.360 -> 0.315 s**;
reordering takes **15.459 -> 0.209 s**;
removing Static takes **14.229 -> 0.210 s**.
The candidate reports the expected hits/misses and performs no Full preparation
for these reuse requests. Each retained quality matches an independent forced
build exactly. The existing Static/Full CLI remains unchanged; legacy Medium
reading is not expanded into a new CLI feature.

Real illumination changes invalidate the stage. Display-only edits refresh it.
Old-producer, changed-code, changed-capture and unsupported-provenance cases
require a bake; verification and actual reuse share the same explicit provenance
guard. Current-source checks and independent forced comparisons pass. A scratch
harness filename collision overwrote its own edited source with a timing receipt
after a successful incremental build; the exact source was reconstructed at a
distinct path and reverified. Other harness setup/expectation corrections are
documented in `harness-corrections.json`; failed receipts remain separate from
the final passing outcomes.

**Verified compressed records.** Project export and ordinary cache seeding now
stage owned one-record ZIP snapshots. The existing bounded decoder validates
the snapshot's exact decoded length, CRC, content hash, Zstd history/ratio and
frame termination before reuse. Publication checks pinned method/CRC/length and
hashes precisely the compressed payload actually copied. It retains bounded
I/O cancellation, decoded aggregate budgets, regular-file checks, source/output
rechecks, sync and atomic publication. Bootstrap manifests remain Deflate.
Existing Stored/Deflate/Zstd streams survive exactly; storage optimization still
re-encodes to apply its requested profile. Package/PLC1 revisions are unchanged.

An existing two-fixture project isolates an incremental metadata export with one
unchanged member. This avoids another large collection rebake. Setup and
correctness checks are excluded from the measured request:

| Producer | Incremental wall s | CPU s | Peak working set MiB | Output bytes |
| --- | ---: | ---: | ---: | ---: |
| Baseline | 0.569 | 0.500 | 20.6 | 3,397,447 |
| Candidate | 0.512 | 0.422 | 21.0 | 3,397,250 |

Common compressed records remain byte-exact, and the complete decoded projects
match across producers after the same limited provenance normalization. Output
size is recorded separately: retaining verified existing codecs does not promise
the same wire encoding/size as recompression. This is one short export pair;
50-ms process polling limits wall-time precision, and no general export speedup
or large-map scaling claim is made. The small wire-size difference includes
provenance/metadata encoding and is not assigned as a compression-size gain.
Complete validation, every member's
`--require-current`, byte-exact editable recovery, no-op repeated export,
direct corrupt-input refusal, CRC cache rejection with byte-exact safe rebuild,
existing-recovery-directory refusal and cooperative
cancellation/restart pass. The tiny Rust raw-copy regression additionally cancels
mid-payload and rejects same-length spool mutation for all three codecs, retaining
accepted output and removing owned partials.

The native player imports the actual candidate project, loads both Full members,
transitions and shuts down normally. This is a bounded format/load smoke check;
no new gameplay, listening or FPS acceptance is implied. No recording, PCM tap,
video or screenshot campaign was produced.

**Final checks and custody.** Format, locked workspace/all-target/all-feature
check, strict workspace/all-target/all-feature Clippy, locked all-feature release,
**321 Rust tests (zero failed/ignored; 1.01 s test execution)**, **24 targeted
Python tests**, asset/texture/prop/audio checks and patch whitespace pass. Two
small essential Rust regressions were added; the existing tiny cache-seed test
was extended for provenance. No shipped map bake enters tests or CI. Assets are
**629 / 389 placeable / nine themes / zero warnings**; the known 100 texture
advisories remain. The broader Python suite was not rerun: its known **17
failures/errors and 16 skips** remain open and are not relabeled as passing.

Evidence is under `C:/Users/Connor Dawkins/Documents/Codex/2026-10-25/task-2/`:
`phase-summary.json`, `paired-bake-results.json`, `lighting-parity.json`,
`variant-results.json`, `invalidation-results.json`, `export-results.json`,
`export-failure-results.json`, `native-export-smoke/receipt.json` and
`quality-final/results.json`. `candidate-bin/` retains four final executables
and six required DLLs, hash-checked against the validated originals. Controls,
sources, unique recovery data and failed harness receipts are preserved.
After publication/remote verification, `cleanup.json` records verified target
cleanup and the observed free-space change; `ownership-handoff.json` records
the exact final commit/remote, clean tree and explicit writer-custody release.

The approximately **2.4-second cold input stall** and **human/subjective ambience
listening review** remain open. Arcade lighting, water, advanced audio, shared AI,
editor work and map construction remain separate queue phases. Mac/Metal,
Linux and the final-version CI gate remain open. No ZIP refresh, upload, release,
version bump, main merge, force push, CI wait, power/security/sleep change,
awake helper or GPU-clock investigation occurred. Connor's manual sleep setting
was left alone.

## Arcade power, screen materials and local light — 2026-10-25

This separate phase starts from remote-verified `bc1f372a65cf5d0c7efe188aabe90127345a55ac`
on `0.13.0-staging`. It fixes the powered-off CRT appearance and calibrates the
Arcade family without changing the queued water, audio, AI, editor or map work.

**Cause and shared fix.** Existing light-owner controls already gate true
emission and their baked/runtime sources. The CRT artwork nevertheless used a
bright diffuse response, so room light left an off display looking powered.
Machine emission and local source output were also independently authored,
with two overlapping cabinet sources totalling 0.54 and reaching 2.4 m. The
north Fighting cabinet's static `material` standby component did not control
its baked prop emission.

Nine adopted GLBs now separate dark CRT glass from powered art. CRT diffuse RGB
is 0.015 and powered emission is 0.65; cabinet marquee diffuse/emission is
0.7/0.4, pinball backglass 0.7/0.45, and pinball lamp faces 0.5/0.8. The
inactive score CRT has zero emission. Printed marquees and backglass can still
be read under independent room illumination. These are artistic engine values,
not calibrated nits or lux. Every GLB binary chunk, embedded PNG, geometry,
normal, UV, alpha contract and nonmaterial document field remains exact. Only
the nine matching adopted-model hashes change in the manifest.

`tools/levels/cabinet_power.py` provides a reusable placement contract using the
existing `light` component: one short 1.8-m source at the measured screen
offset, intensity `0.14 * cabinet_width / 0.82 * emission_scale`, authored
palette retained. An off or dimmed/switchable machine uses the same owner for
luminous slots and attached light sources. Fully powered uncontrolled copies
retain draw merging. The Arcade generator and all 36 current cabinet placements
use it; north Fighting's actual owner now scales both emission and source to
0.16. Other props, room/decorative fixtures, scripts, quests, transitions and
level fields compare exactly. Independent neon/UV/decorative lighting is retained.

The renderer indexes emission controls by owner. A switch writes only changed
existing luminous material uniforms for that owner; the frame loop visits only
authored active animations. Steady machines require no per-frame emission scan.
Power-off dominates animation. There is no new rebake, whole-mesh upload, prop
scan, texture generation, lighting invalidation or geometry admission change.
Two tiny CPU regressions protect off/animation/repeated-toggle behavior and
independent luminous identities while keeping nonemissive hardware shared.
They use no GPU, map bake or asset fixture.

**Native visual and circuit evidence.** Actual before/after PNGs were inspected
at 1280x720. The five-cabinet fixture covers dark and normally lit on/off views,
repeated toggles, separate room illumination, nearby static chairs, and Full ->
Static -> Full restoration. Full-off and repeated on/off images are pixel-exact
within each run. In matching screen regions, normally lit off mean encoded RGB
falls from 13.46–36.45 to 0.62–5.65; these are image code values, not luminance.
Powered art stays readable with reduced glow and localized spill. Both dark-off
runs were already dark: the principal defect was room-lit diffuse artwork.
The native nearby-body probe has zero machine direct strengths and zero indirect
HDR input with the room and machines off; both become nonzero when powered.
Its GPU vertex/index allocation, geometry revision and visibility mesh-build
count remain stable through the switches. Independent neon stays visible.

Separate fixtures cover both pinballs, active/inactive score CRTs, an authored
off machine, and nearby dynamic-prop lighting. Saved authored-off source compiles,
imports, loads and reloads off. Runtime state also survives the Full/Static/Full
quality rebuild. Static quality retains its existing authored-light-state
contract; runtime source switching is a Full-quality feature. This is source
save/load coverage, not a claim of arbitrary runtime power persistence in a
savegame. The initial/reload camera pitches differ, so those two images are not
claimed pixel-identical.

The exact final player reruns all four primary fixture cases. Cabinet, authored
off and pinball captures match the initial candidate 8/8, 7/7 and 8/8; nearby
case power views match 6/6, with its two later decorative look-at views differing.
The larger scene uses the actual whole Arcade geometry/lights with its real
Lua script in an isolated one-level project; only the hub transition/return
binding is removed from this inspection harness. Before/after north standby,
wall row, scoreboards and pinballs were captured and inspected. North Fighting
is visibly dimmer, the fourth inactive score monitor is black, and powered
low-poly art remains legible. Both scene runs shut down normally with no
captured script diagnostic. Geometry GPU bytes stay 31,088,824 vertex /
701,952 index. The independent standby owner adds one batch and five draw
ranges (64/177 -> 65/182); uploads stay 24 with one additional cache hit.
This scene uses prepared fallback geometry in both players (canonical instance
counters are zero), so it is not positive native instancing evidence. The GLB
binary comparison and existing canonical-geometry regressions cover preservation;
no claim of unchanged unique texture memory is inferred from per-batch counters.

The bundled menu is rebuilt from unchanged `projects/menu` source and passes
typed validation and current-input verification before installation. It is
16,420,636 bytes, SHA-256
`8662048651aa47670b5f25923406b9137306a9107dfe49bed57e2c11a43870d3`.
A quiet final-player run captures repeated true all-off menu beats without
script diagnostics; the inspected blackout has no powered CRT art. The menu's
existing independent room fixtures and Full-only runtime contract are retained.

Reflection acceptance remains **inconclusive**. Native planar passes execute,
but the tested floor and wall views do not show a recognizable reflected CRT
even when powered. The same wall setup is blank with the baseline `bc1f372a`
player and with diagnostic occlusion disabled. The common material-uniform path
is retained; this phase does not claim a verified visible CRT reflection or
expand into repairing the broader reflection visibility issue.

**Uncapped native timing.** Each scene has three before/after pairs in
alternating order, 15 seconds after ready with 120 warmup frames, real frame
delta and no screenshots/readbacks. All use High quality/filtering/lighting,
Full lightmaps, Full reflections and bloom, 1280x720 physical drawable,
DX12/NVIDIA RTX 3070 Laptop GPU, VSync off, no frame limiter and actual
`Immediate (immediate)` confirmed after initial surface creation. Every run
exceeds the 165-Hz display rate; these samples are not refresh-rate plateaus.
No compiler/build job runs during them.

| Scene | Median FPS before -> after | FPS range before / after | Median p95 loop ms before -> after |
| --- | ---: | --- | ---: |
| Five-cabinet fixture | 353.10 -> 355.95 | 348.97–359.76 / 350.40–356.45 | 5.841 -> 5.744 |
| Actual Arcade, fixed view | 313.17 -> 318.04 | 310.62–313.71 / 317.60–321.75 | 4.574 -> 4.516 |
| Menu Arcade vignette | 271.58 -> 271.16 | 269.79–271.85 / 268.76–273.14 | 6.964 -> 6.912 |

These are bounded fixed-view comparisons, not a general speedup or exhaustive
roaming/no-regression claim. Menu samples force Arcade vignette 4, seed 42;
the menu comparison also includes bringing its old bundled scene current.
Capture-time BENCH_SUMMARY output is excluded. Large-map bake timings overlapped
build work and establish no bake-performance result.

**Validation and evidence.** Final format, locked workspace/all-target/all-feature
check, strict Clippy (`--workspace --all-targets --all-features -- -D warnings`),
locked all-feature release, aggregate Rust tests, targeted Python tests,
asset/texture/prop/audio validation and patch whitespace pass. There are
**323 Rust tests**, zero failed/ignored, and **24 focused Python tests**.
Assets remain **629 / 389 placeable / nine themes / zero warnings**; 140 texture
assets retain the known 100 advisories, and prop checks retain existing art
advisories. Audio remains 28 cues / 66 WAVs / 10,855,264 bytes / 41.40 MiB
decoded. The broader Python suite was not rerun; its known **17 failures/errors
and 16 skips** remain open and are not reported as a clean full suite.
All six candidate fixture/Arcade/menu packages receive final-compiler typed
validation and `--require-current` verification.

Evidence is under `C:/Users/Connor Dawkins/Documents/Codex/2026-10-25/task-3/`:
`phase-summary.json`, `material-tuning.json`, `source-audit.json`,
`screen-pixel-audit.json`, `receiver-audit.json`, `native-final-acceptance.json`,
`arcade-native.json`, `fps-results.json`, `fps-summary.json`,
`package-final/results.json` and `quality-final/results.json`. Native PNGs and
logs, exact source fixtures, controls and recovery data are preserved there.
`final-bin/` holds four final executables and six DLLs, hash-checked against
validated originals; player SHA-256 is
`66754e33e46dbdecd345c85774623bcf4f40f3509228c06b9f532e9cc5c2318a`.
`preserved-binaries-final.json` and `deliverable-hashes.json` record identities;
`cleanup.json` and `ownership-handoff.json` record final target cleanup, exact
commit/remote, clean tree and writer-custody release after publication.

Failed exploratory receipts are retained separately. An early contended menu
run exhausted the Lua budget and is excluded; the quiet final run passes.
An initial Arcade harness omitted its script manifest; it was corrected using
the real script/project context and the compiler's normal verified preparation
cache hit, without a rebake or validation bypass. Initial Clippy caught one
test-helper shadowed name, which was fixed before final gates. A refused
hardlink attempt was replaced by ordinary independent backup copies. Reflection
diagnostics and the corrected camera/pixel-comparison scopes remain explicit.
Automatic approval review denied edits to the two canonical asset/map guides
because this phase permits updating only this release report; neither guide
was changed and that action was not retried. The material/authoring contract
is recorded above instead.

The approximately **2.4-second cold input stall**, **subjective ambience
listening**, reflected-CRT visual acceptance, Mac/Metal/Linux checks and the
final-version CI gate remain open. No ZIP refresh, recording, comparison video,
Library upload, version bump, release, main merge, force push, CI wait,
power/security/sleep change, awake helper or GPU-clock/charger investigation
occurred. Connor's manually configured sleep setting is unchanged.


## Initial water candidate: performance rejected (Windows, 2026-10-25)

The initial implementation was pushed as
`259c2671eda1ed929b54e3b09215d7910922de6e`. Its visual/interaction work and evidence
are preserved, but the measured Beach and repeated-drop GPU regressions below
were rejected. The water phase remains open for the correction described after
this historical record.

This phase starts at clean staging commit
`fca71d907155f14fb188786264f511fee2260139` and implements the four approved
water improvements without changing mean surface height, swimming, collision,
current forces or buoyancy. Source/PS2 material art remains the authored PNGs;
no texture or shipped map asset changes, fluid simulation, new planar pass or
unrelated reflection-system change is introduced.

**Presentation.** Existing UV motion now has more visible analytic normal motion.
Water uses Schlick reflectance (F0 0.02037) instead of the ordinary gloss mix;
incident lighting is reused, with the existing authored reflection scale retained.
Medium/High use opaque view-ray thickness, bounded at 12 m, RGB absorption plus
existing authored attenuation, and refraction displaced by at most three scene
pixels. Every texel of the bilinear footprint must pass foreground-depth and
above-water/viewport checks; failure samples the original pixel. Colour filtering
uses the existing immutable clamped-linear sampler. Compensated straight alpha
retains already sorted transparency. This is opaque-scene refraction, not correct
multi-layer transparent ray tracing. Low and reflection captures retain the
ordinary blended fallback.

A 5.5 cm ray-thickness contact fade and weak dark meniscus replace a sheet-like
hard contact; no indoor ocean foam is generated. Existing authored Beach foam
is unchanged. The water section of `docs/MAP_AUTHORING_GUIDE.md` records the
changed appearance and fallback contracts. Material/image contracts are unchanged.

**Interactions and bounded work.** Changed player footprints, rigid props and
authored floating props feed the same model-independent impulse path. Real entry,
throws and slow duck bobbing emit local expanding normal rings/wakes; settled,
dry, fully submerged and teleported subjects do not. The final optical budget is
**eight events**, with a 2.8-second maximum lifetime and priority for stronger,
recent impacts. Ambient zero-motion water can still react to an impact. Events
stay in their authored water volume. Normal motion never displaces the plane.
Per-subject cooldowns and a bounded indexed motion queue coalesce repeated poses;
no per-frame scenery scan or pose-driven mesh upload is added. CPU-prepared
annuli/decay reject irrelevant pixels before expensive ripple arithmetic.

Opaque inputs use one reusable RGBA16F/Depth32 pair at scene resolution. A cached
water bound uses the existing conservative frustum/occlusion result; potentially
visible water copies a projected colour rectangle with a five-texel guard, plus
a separate full depth copy. Hidden views reuse allocations; Low, unload and
level replacement retire the snapshot/bindings/history. No input aliases a live
attachment. Water optics specialize out of opaque/cutout programs, and water
Fresnel is calculated once and reused.

Moving-body caustic visibility now prunes a static receiver hierarchy, composes
only dirty cells in the original source/FMA order, and transfers coalesced spans
in at most **64 queue writes per frame**. Light-source changes retain their
existing full composition; ordinary prop movement does not rebuild the water,
receiver field, meshes or lightfield. Existing source-type eligibility and energy
rules remain intact. In the native repeated-drop checkpoints full-field uploads
stay at one, whereas baseline rises from five at held pickup to 232 after drops.
Regression fixtures replay sparse transfers bit-for-bit against full composition,
including a fragmented 200-range case, without full-map baking.

**Native visual/functional evidence.** All final checks use the preserved final
player, SHA-256 `d0afcbe2fcd48ea04ab6e4ad8f1ab1c6dc9574c95dbf8f740e56866f397a219a`. Seven fixed-view
before/after PNGs cover Pool from above, two motion times, near grazing, ladder/
foreground, submerged, circular contact, and Beach shore/depth. Pool uses Full
lightmaps; Beach's package supports Static. Both sides use High quality/filtering,
Full reflections and bloom, 1280x720 physical drawable, VSync off and actual
Immediate presentation. The final image review finds clearer depth/absorption,
angle response and restrained motion, with intact ladder/shore silhouettes.
The four checked dry/foreground ROIs are pixel-identical to baseline. Optimized
sampling differs from the full manual-sampling trial by at most one or two RGB
levels in these views. Temporal water changes include existing caustics, so their
pixel deltas are not presented as an isolated normal-motion measurement.

The small three-bay fixture contains rigid ducks/drums and three authored floating
ducks. Baseline and final player use the exact same package and action sequence
for a real crouch/pickup/recapture/mouse throw (one `item_thrown` event), three
repeated duck/drum drops, player entry, Low fallback and High recovery. All ten
final captures are inspected. Additional matched native PNGs show the shipped
Pool's actual duck/drum entering and settling near the camera. Capture-time FPS
is excluded. Three final load/menu-return cycles pass, including Low/Medium/High,
960x540 -> 1280x720 recovery, stable solver/body counts and clean shutdown.
The final compiler rebuilds only the small fixture; typed validation and
`--require-current` verification pass. Its SHA-256 is `f6205e99a541235c46c720985a88787e616af3d521979c04f88f0681d0c2e6dc`.
The representative project packages stay unchanged for matched runtime evidence;
this does not claim a final-version rebake or ZIP refresh.

**Performance and tradeoffs.** The first 32-event candidate is rejected: its
three-pair fixed-drop median falls 458.90 -> 312.66 FPS, despite lower CPU update
work. Prepared annuli/filtering alone still produce 465.53 -> 319.46 FPS in a
separate diagnostic probe. These receipts and binaries are retained. The final
version uses the explicit eight-event optical budget, shared reflectance and
guarded hardware filtering, without lowering quality elsewhere. It still incurs
a measured GPU cost for the requested optics; **this is not an overall FPS
speedup**. CPU water-lighting work improves, while large water coverage and local
optics can increase total frame time. These costs are reported rather than
hidden by a limiter, changed resolution or reduced unrelated settings.

Final primary measurements use three alternating before/after pairs per case,
12 seconds after ready and 120 warmup frames (14 seconds for the true-throw/entry
case), real delta, High/filtering/Full reflections/bloom, 1280x720 physical,
DX12/NVIDIA RTX 3070 Laptop GPU. Full lightmaps are used except Beach Static.
VSync is off, no frame limiter is present, and actual `Immediate (immediate)` is
verified after initial surface creation. Every run exceeds the 165-Hz display;
no refresh-rate plateau is accepted. No compiler or other player overlaps them.
Primary runs have no screenshot/readback or CPU/GPU/resource trace; physical
cases log identical physics CSVs on both sides. Separate diagnostic replays carry
CPU/GPU/resource traces. Figures below are medians of per-run statistics.

| Scene | Median FPS before -> after | p95 / p99 loop ms before -> after | p95 CPU update ms before -> after | p95 GPU pass ms before -> after |
| --- | ---: | --- | ---: | ---: |
| Repeated drops, fixed view | 469.08 -> 351.95 | 2.677 / 3.193 -> 4.526 / 5.078 | 1.456 -> 1.324 | 1.202 -> 2.787 |
| True throw, drops and player entry | 484.13 -> 393.03 | 2.587 / 3.259 -> 4.360 / 4.813 | 1.459 -> 1.173 | 1.096 -> 2.671 |
| Beach, fixed view | 313.87 -> 243.85 | 4.932 / 5.384 -> 5.315 / 5.711 | 0.556 -> 0.655 | 3.287 -> 4.230 |
| Pool, fixed view | 288.29 -> 273.12 | 6.344 / 6.817 -> 5.877 / 6.281 | 0.306 -> 0.308 | 4.155 -> 3.542 |
| Dry Arcade control | 319.83 -> 318.85 | 4.474 / 4.834 -> 4.458 / 4.841 | 0.976 -> 0.991 | 3.785 -> 3.807 |

The seeded visible-water menu measures 269.15 ->
260.03 FPS over another three alternating pairs.
That view contains Pool water and is not a dry control; its early 275 -> 255 FPS
trial was initially misclassified and corrected after inspecting the native PNG.
The dry Arcade case above guards unrelated rendering. Fixed-drop mean CPU update
is 1.156 -> 0.981 ms;
GPU pass p99 is 1.467 -> 3.688 ms and median-run
worst loop is 7.157 -> 6.471 ms.
GPU pass unions exclude copy/barrier gaps; end-to-end loop/swap tails include them.
Raw per-run p95/p99/worst, sample counts and measured scope remain in the receipts.
The controlled drop and scripted throw/entry workloads are distinct and are not
combined into a game-wide throughput claim.

**Allocation and resource evidence.** New motion/ripple and dirty-cell work uses
preallocated bounded storage; essential tests cover burst coalescing, capacity,
hide/despawn/clear, expiration and replacement. Final native active events never
exceed eight, and histories reset on unload. Snapshots are 11,059,200 texel bytes
at 1280x720, 2,764,800 at Medium's 640x360 scene and 6,220,800 at 960x540; Low and
unloaded-level rows report zero. The menu can create its own fresh snapshot.
Loaded fixture texture counts/bytes remain stable across three cycles; geometry
budgets do not grow per interaction. HAL counters include alignment where
available, and texel budgets exclude driver overhead; neither is complete VRAM.
Transient buffer/pool variation is retained in raw data, not called a leak-free
proof for the whole process.

The existing harness has no Windows RSS sampler. A task-local read-only limited
query handle samples Windows working set/private bytes every 250 ms and records
OS peak working set/commit plus CPU times, without installing a dependency or
reading process memory contents. Peaks include startup/steady state and are not
Rust allocation-call counts or GPU VRAM. In the fixed-drop runs maximum peak
working set is 357.30 ->
363.05 MiB, peak committed memory
491.46 ->
495.84 MiB. Full lifecycle process peaks,
per-run samples and source capacity evidence are retained for interpretation.

**Gates and handoff.** Final strict Clippy, locked all-target/all-feature check,
locked workspace/all-feature release, aggregate **328 Rust tests** (zero failed/
ignored), **25 focused Python tests**, asset/texture/prop/audio checks, format
and patch whitespace pass. The five added Rust regressions use tiny CPU-only
fixtures with stated unique value; no default test bakes a shipped map. Assets
remain 629 / 389 placeable / nine themes / zero validation warnings; 140 texture
assets retain 100 known advisories. The broader Python suite was not rerun;
its **17 known failures/errors and 16 skips** remain open.

Evidence is under `C:/Users/Connor Dawkins/Documents/Codex/2026-10-25/task-4/`:
`phase-summary.json`, `performance-final-analysis.json`, `final-timing-results.json`,
`final-menu-results.json`, `final-diagnostic-results.json`,
`resource-analysis-final.json`, `visual-pixel-analysis-final-budget.json`,
`quality-final-budget/results.json`, final native PNGs/logs, controls and source
fixtures. Earlier rejected/exploratory receipts are preserved. Initial native
harness misses (falling pickup target, mouse recapture and persisted pause-menu
selection), a clipped synthetic occluder and caught compile/lint issues were
corrected; those failed receipts are not acceptance evidence. No automatic
approval-review denial remains in this phase.

`final-bin/` preserves four production executables and six DLLs outside target,
hash-checked against validated sources. `deliverable-hashes.json`, `cleanup.json`
and `ownership-handoff.json` record preservation, final target cleanup, exact
commit/remote, clean staging tree and explicit writer-custody release.

The ~2.4-second cold input stall, subjective ambience listening, reflected-CRT
visual acceptance, broader Python failures, Mac/Metal/Linux validation and final
version CI/package gate remain open. The measured High-water GPU cost above failed
acceptance and triggered the performance correction; it is not an accepted appearance tradeoff. No advanced audio, AI, editor or Backrooms work,
ZIP refresh, video/comparison recording, Library upload, version bump, release,
main merge, force push, CI wait or power/security/sleep change is performed.
Connor's manually configured Never-sleep setting remains unchanged.

## Water performance correction: acceptance still open (Windows, 2026-10-25)

The initial `259c2671` water candidate above failed performance acceptance. This
forward correction preserves its four requested appearance/interaction changes
and recovers the large Beach and throwing throughput losses. **It does not close
water acceptance:** High interaction frame tails remain worse than the pre-water
baseline. That cost has not been accepted as an unavoidable visual tradeoff.
Audio and the other subsequent phases have not started.

**Implemented correction.** The water transparency pass now samples the existing
scene depth through a read-only depth attachment. Opaque/capture/emission passes
retain a separate fallback camera binding, so the depth cannot simultaneously be
bound for sampling and writing. Snapshot bindings follow actual texture-view
identity, including same-size target replacement; resize, quality changes,
unload and reload retire the old views. Opaque diagnostic probe draws execute
before read-only transparency. Only the conservative water colour rectangle is
copied. The full-size reusable colour allocation remains stable as the camera
moves; the extra full-size depth texture and copy are removed.

Unambiguous water surfaces cache their canonical tree leaf at world upload. The
cache reuses the existing vertex tag, adds no mesh bytes and performs no per-frame
global scan or mesh upload. Ambiguous overlaps, circle/rectangle boundaries and
large authored sets retain the complete original priority query. Shared corners
retain their first valid hint rather than overwriting a large pool triangle with
an adjacent bay's hint. Water is selected once and reused by motion/compositing;
opaque/cutout pipelines specialize all water branches out. No planar reflection,
fluid simulation or unrelated reflection-system change is introduced.

The original bounded eight-event ripple/wake budget, decay/retirement, collision
and buoyancy, mean water height, Low/reflection fallbacks, foreground depth guards,
angle-dependent reflection and depth absorption remain active. The earlier
coalesced moving-body lighting updates and per-prop mesh-upload fixes remain.
Disabling ripples/optics, weaker depth guards, unrolled loops, an extra depth copy
and broader visibility upload batches were diagnostic trials, not shipping
defaults. The upload-eight trial saved calls but transferred 43% more bytes and
did not consistently improve true-throw tails; it was rejected and the checked
source restored byte for byte. The undefined-identifier shader error in an
isolated diagnostic helper was fixed once with a function-bounded replacement;
the failed receipt is preserved and is not a validation result.

**Matched performance.** The corrected primary campaign has 30 runs: three
alternating before/after pairs for five scopes, against the preserved `fca71d9`
player. Settings remain High/filtering/Full reflections/bloom, DX12, RTX 3070
Laptop GPU, 1280x720 physical. Full lightmaps are used except Beach Static.
VSync and frame limits are off; actual `Immediate (immediate)` is verified and
every run exceeds the 165-Hz display. Each run measures 12 seconds after ready
with 120 warmup frames discarded, or 14 seconds for true throw/drop/player entry.
The physical cases log the same physics CSV scope on both sides; a real mouse
throw occurs exactly once in every true-throw run. Primary timings exclude
screenshots and detailed CPU/pass/resource traces. Separate diagnostic replays
measure pass labels/resources. Figures are medians of per-run statistics, not
pooled percentiles or comparisons across earlier timing epochs.

| Scope | FPS before -> corrected | Loop p95 / p99 ms before -> corrected | GPU p99 ms before -> corrected |
| --- | ---: | --- | ---: |
| Repeated drops | 445.05 -> 425.50 | 2.774 / 3.480 -> 3.734 / 4.479 | 1.406 -> 2.664 |
| True throw, drops and player entry | 457.50 -> 460.14 | 2.701 / 3.079 -> 3.142 / 4.290 | 1.190 -> 2.374 |
| Beach | 308.63 -> 322.24 | 5.093 / 5.539 -> 4.801 / 5.349 | 3.702 -> 3.720 |
| Pool | 292.05 -> 279.66 | 6.424 / 7.158 -> 6.599 / 7.238 | 4.919 -> 4.826 |
| Dry Arcade control | 317.47 -> 328.73 | 4.508 / 4.909 -> 4.237 / 4.681 | 3.835 -> 3.652 |

Another three alternating pairs in the seeded visible-water Pool menu measure
268.35 -> 284.85 FPS, loop p95 6.699 -> 6.010 ms and p99 7.432 -> 6.437 ms.
The menu contains water and is not a dry control. Repeated-drop mean CPU update
falls 1.217 -> 0.929 ms and p95 1.559 -> 1.289; true-throw mean falls
1.203 -> 0.909 ms and p95 1.558 -> 1.241. Mean GPU time increases
0.684 -> 1.140 ms and 0.536 -> 0.922 respectively. The remaining interaction
p99 loss is meaningful despite recovered mean throughput. This correction is
therefore a reviewable improvement, not a claim of baseline-tail parity or
completed water acceptance. Pool mean throughput is also 4.2% lower in this
campaign and remains part of the reported tradeoff.

Detailed per-pass tracing of the corrected repeated-drop replay samples mean
scene 0.488 ms and water transparency 0.541 ms. Those sparse label samples are
diagnostic; they do not replace the full-run GPU/loop tail measurements, and no
missing baseline pass labels are fabricated. GPU pass sums omit copy/barrier
gaps; loop and swap measurements retain their end-to-end cost. The retained
shader/upload ablations do not establish that the remaining cost is unavoidable.

**Visual and lifetime checks.** Seven final native views (Pool above, motion,
grazing, ladder/foreground, submerged, round contact edge and outdoor Beach) are
inspected and pixel-identical to the initial four-feature candidate: zero changed
RGB pixels in all seven. This establishes that the measured correction did not
obtain its gains by degrading those views. Ten further native fixture captures
cover real throwing, repeated impacts, player entry, floating ducks/wakes and
Low-to-High recovery. Three load/menu-return cycles, Low/Medium/High changes and
960x540-to-1280x720 recovery pass; five lifecycle screenshots are inspected.
Probe Field, Probe Neighborhood and restored Final views are inspected after
the read-only pass change, with clean shutdown and no GPU validation error.
These screenshot/trace runs make no FPS claim.

At matched loaded checkpoints across all three cycles the fixture has 39 HAL
textures, 84,082,688 HAL texture bytes and 84 buffers. Each unload reports zero
water snapshot bytes, active events and tracked bodies. High snapshot texel
storage falls from 11,059,200 to 7,372,800 bytes (one third less); Medium is
1,843,200, 960x540 is 4,147,200, and Low/unloaded is zero. The menu can allocate
its own new snapshot. Active events remain capped at eight; histories use the
existing preallocated track capacity of 448. HAL counters include transient
uploads/capture readbacks and pending worlds; they are partial GPU memory
evidence, not complete VRAM or a whole-process leak proof.

Read-only Windows limited-query sampling retains working-set/private/commit
peaks and page-fault counts. Repeated-drop maximum peak working set is
357.30 -> 343.17 MiB and peak commit 492.14 -> 485.64 MiB in this campaign;
these include startup and are not Rust allocation-call counts. Raw per-run
CPU/GPU tails, worst frames, samples and other process-memory peaks remain in
`performance-correction-analysis.json` and the receipts.

**Validation and preserved deliverables.** The selected corrected source passes
strict workspace/all-target/all-feature Clippy, locked all-target/all-feature
check, locked workspace/all-feature release, **329 aggregate Rust tests**
(zero failed/ignored), **25 focused Python tests**, asset/texture/prop/audio
checks, format and patch whitespace. The one new tiny CPU regression protects
cache tree ownership, adjacent/overlap classification and its preparation cap; it
requires no full shipped-map bake. The final small water fixture compiles,
validates and passes `--require-current` verification; SHA-256 is
`74575a623cfbd3f735643ea56efc07302618bca3427a128a510684b56a6f4fad`.
Shipped packages remain frozen for matched views and are not final-version
rebakes. Asset counts/advisories are unchanged. The 17 broader Python
failures/errors and 16 skips remain open, without another broad rerun.

Correction evidence is in
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-25/task-4/water-performance-correction/`:
`phase-summary-corrected.json`, `corrected-timing-results.json`,
`performance-correction-analysis.json`, `corrected-menu-results.json`,
`corrected-visual-equivalence.json`, `resource-analysis-corrected.json`,
native receipts/PNGs, diagnostic trial sources and logs, and
`quality-cached-copy-fixed/results.json`. The exact checked source was restored
after the rejected upload trial; `source-restoration-verification.json` proves
the byte-for-byte patch/test match, so the selected build's checks and native
results remain applicable. The selected player SHA-256 is
`044b3fc5851d75f57a01fb02893745fbc20bbe1db14a141c7438a9613d805509`.
`cached-copy-bin/` preserves all four checked production executables and six
DLLs outside target. Rejected upload-eight binaries are separately labelled.
`preservation-verification.json` verifies every entry of the original evidence
manifest and all selected binaries; the original evidence is preserved.
Correction-local handoff/hash/cleanup receipts record the eventual staging
commit, remote verification, tree state and writer custody.

The High interaction tail regression remains an explicit acceptance blocker.
The ~2.4-second cold input stall, subjective ambience listening, reflected-CRT
visual acceptance, broader Python failures, Mac/Metal/Linux checks and final
version gate also remain open. No subsequent queue phase, ZIP refresh, video,
Library upload, release, version bump, main merge, force push, CI wait or
security/power/sleep setting change is performed.

### Remaining water tails: focused attribution, acceptance still open

The follow-up to `2a2ffa9f` retains all four water improvements and its depth,
ownership, CPU-lighting and mesh-upload wins. No further quality-preserving
runtime optimization was established. The remaining High interaction tail is
reproducible; it is not dismissed as benchmark noise or accepted as unavoidable.
The exact split within the optical shader's texture operations, arithmetic and
GPU scheduling remains unresolved. No reduced-effect default is shipped.

The diagnostic changes make that conclusion reviewable. Raw GPU traces now
include valid scenes with fewer than five passes and optionally select slow
samples, retaining the hard 64-record cap. The trace budget has its own small
owner, independent of timestamp decoding and mapping lifetime. At most 96
explicit swap-stage records split presentation from device polling. Separate
benchmark-only controls expose queue depth, DX12 acquisition wait and the water
optical fallback. Normal play retains queue depth 2, DX12 Wait and full optics;
these controls create no per-prop resource or per-frame global scan. All seven
core water implementation files remain byte-identical to the preserved
`2a2ffa9f` source. No water shader/texture, reflection system, collision,
buoyancy, mean height or quality default is changed by this follow-up.

**Attribution.** Existing primary replays were joined with the correct CPU
alignment: a row's loop cadence covers the preceding frame's work. Slow
corrected frames correlate most strongly with that preceding presentation wait
(0.80-0.88), while CPU update time is lower. GPU samples arrive asynchronously;
they are not treated as measurements of the CPU row carrying them. Most tail
frames are outside the first 100 ms after an item-pose event.

Twelve new timestamp-on/off controls rule out timestamp collection as the
explanation: full-path median p99 is 4.453 ms with timing and 4.461 without it.
Eight matched normal/forced-completion controls show scheduling sensitivity:
the before/after p99 gap shrinks from 3.149 -> 4.507 ms normally to
3.203 -> 3.422 ms with explicit completion. Completion reduces throughput,
so it is rejected as a shipping fix. Nine same-build queue-depth controls reject
depth 1 (about 242 FPS, p99 4.847 ms); depth 3 provides no useful improvement
over depth 2. Six same-build DX12 Wait/DontWait controls show only
4.545 -> 4.468 ms p99, so the normal policy is retained.

Sixty-four selected slow GPU samples locate a median 1.422 ms opaque scene span,
0.672 ms water-transparency span and 0.115 ms gap between pass spans. Those are
elapsed GPU timeline spans, including scheduling, not independent active-work
costs. The actual slow CPU swap records are predominantly presentation:
median 2.235 ms presenting and 0.247 ms polling. Static/entity shader and
diagnostic-hook ablations show no useful gain. Preparing fixed camera basis and
coordinate scales produces only 4.524 -> 4.463 ms p99 in three pairs, so no
uniform/shader change is retained on that evidence.

The decisive optical controls use the same CPU executable code, geometry,
physics actions, lighting and High settings. An isolated, equal-length embedded
WGSL substitution removes only the final `water_composite` branch. It retains
the snapshot, separated pass, ambient motion, Fresnel and interaction normals.
A third control also removes snapshot/pass setup. These copied executables are
explicitly diagnostic, hash-labelled and never production deliverables.

Three balanced runs per path measure 12 seconds after ready with 120 warmup
frames excluded, High/filtering/Full reflections/bloom/lightmaps, 1280x720
physical, real delta and the same physics CSV scope. Actual Immediate is
verified, VSync/frame limits are off and every run exceeds the 165-Hz display.
Primary runs have no screenshots or detailed CPU/GPU/resource tracing. The
additional 3.2-12-second segment starts at the first repeated drop and is
reported alongside the complete interval, rather than replacing it.

| Same-build optical path | Whole-interval FPS | Whole p99 ms | Repeated-drop p99 ms (3.2-12 s) | Mean CPU update ms | Mean / p99 GPU span ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Full selected default | 417.68 | 4.467 | 4.211 | 0.938 | 1.163 / 2.776 |
| Optical shader ablated; snapshot/pass retained | 477.37 | 3.765 | 2.988 | 0.943 | 0.782 / 1.938 |
| Optical shader and snapshot/pass ablated | 498.21 | 3.308 | 3.001 | 0.936 | 0.702 / 1.405 |

CPU update averages differ by less than 0.007 ms. The shader ablation removes
the repeated-drop tail penalty while retaining all CPU interaction work,
ripples and snapshot/pass setup. Removing setup further improves average
throughput but does not improve that steady p99. This attributes the remaining
tail primarily to the requested optical path and its presentation/scheduling
interaction, not a new per-prop CPU rebuild, unbounded events or mesh uploads.
Whole-interval tails also include startup/held-target behavior; their variation
is retained above. Percentiles and GPU pass spans are not additively decomposed.

**Concrete choices, not silent defaults.** Keeping the current full path
preserves all four approved improvements at the measured cost. A compiled
water-only optical fallback is the measured lower-cost alternative: it retains
motion, angle-dependent surface reflection, local ripples and physics, but loses
guarded refraction, view-ray absorption and depth-based contact/meniscus. It
therefore does not satisfy the full approved visual scope. The 498-FPS control
also specializes the shader; the runtime `--no-water-optics` switch alone is not
claimed to provide that result. A different capture/shader architecture could
preserve the visual scope, but no gain for such a redesign is established by
these measurements. Full optics remain selected and acceptance remains open;
this evidence is not a proof that every possible implementation has this cost.

New evidence lives under
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-25/task-4/water-tail-attribution/`:
`final-attribution-analysis.json`, `existing-frame-attribution.json`,
timestamp/completion/queue/wait summaries, `slow-pass-attribution.json`,
`setup-primary-results.json`, diagnostic provenance and raw receipts. Prelaunch
helper omissions, the unusable no-presentation control (loading waits for a real
presentation), and the initial caught diagnostic-helper lint failures remain
preserved as failed receipts, not acceptance evidence. The cold input stall, listening/CRT
acceptance, broader Python failures, platform checks and final version gate
remain open. Audio and all subsequent queue phases remain untouched.

Final strict Clippy, locked workspace/all-feature release, aggregate **329 Rust
and 25 focused Python tests**, asset/texture/prop checks, format and whitespace
pass. Six invalid diagnostic argument combinations reject before native launch.
The initial Clippy failures were corrected by extracting the bounded trace owner,
using distinct binding names, typing the threshold and ordering declarations;
no lint suppression is added. No new persistent test suite or shipped-map bake
is introduced. The 17 known broader Python failures/errors and 16 skips remain
open without another broad rerun.

Post-final-source native checks inspect two matched Pool views (above and
ladder/foreground): both are pixel-identical to the preserved full-quality
candidate. Five quality/resize screenshots are inspected across three complete
load/menu-return cycles. Each matched loaded checkpoint has 39 HAL textures,
84,082,688 texture bytes, 84 buffers, seven bodies and 453 colliders; each unload
has zero water snapshot bytes, active events and histories. Active events remain
bounded at eight. Final traces retain exactly 64 slow full-path records and 64
valid four-pass fallback records, verifying the cap and removal of the old
five-pass blind spot. Screenshot/resource/trace runs make no FPS claim. HAL
counters remain partial GPU memory evidence, not total VRAM or allocation-call
counts. The final small fixture compiles, validates and verifies current; its
SHA-256 is `7fe786818bf1783dfd3699a754b2bf359c6c7e7fb671af627a2b76a2a2544d26`.

`quality-final-fixed/results.json`, `final-native-functional.json` and
`final-native-analysis.json` identify these final checks. Selected production
executables/DLLs are preserved in `final-bin/`; the player SHA-256 is
`19a4a3a22718e5f340919e8281e2c9aae9fbab4e260244493ab75e258ecdf0ba`. `final-runtime-source.patch` records the exact checked/native-tested
runtime changes. Correction-local preservation, handoff and cleanup receipts
record the staging commit/remote, clean tree, retained evidence and idle-only
Cargo cleanup. Full visual quality remains selected; the measured optical tail
and the unresolved instruction-level attribution remain open. No audio or
subsequent queue phase is started.


### Spatial audio, material acoustics and sound-system audit (#32)

This completes the implementation and available Windows technical validation of
[issue #32](https://github.com/csd113/Places/issues/32), based on
remote-verified `a863b3550fec4b085ed0fc0cf9e92b314f3e7b1c` on `0.13.0-staging`.
Subjective listening/localization and unavailable native-platform coverage
remain explicitly unverified below; they are not converted into checked
listening acceptance.

The user accepted retaining all improved water optics and moving to audio after
the preceding investigation. That decision supersedes the earlier wording that
water acceptance was still awaiting a choice. The selected full optics retain
the known **4.211 ms repeated-drop p99**, versus **2.988 ms** in the
shader-ablated diagnostic. This audio work neither changes those optics nor
claims to fix that residual, the separate approximately 2.4-second cold input
stall, or reflected-CRT visual acceptance. No water shader, renderer or water
simulation file changes.

**Implementation.** The existing Kira/SDL mixer, listener, entity anchors,
distance attenuation, stereo panning and quiet cue assets remain shared. Audio >
Output persists Speakers (the compatible default) or Headphones independently of
graphics quality. Headphones use measured diffuse-field MIT KEMAR responses: 368
stored half-plane directions, mirrored ears, three-direction interpolation on
the game thread and complete 140-tap stereo FIRs at 48 kHz. The complete
128-tap/44.1-kHz source responses are resampled offline; no
tap/sample-rate/voice quality reduction was used to meet the budget. Generic
head fit and elevations below the -40-degree measurement floor are limitations.
`KEMAR.txt` attributes Bill Gardner and Keith Martin, MIT Media Laboratory,
1994, and preserves its free-use-with-attribution terms (not the MIT software
license). Source archive SHA-256 is
`a48e73137390626ef1e45d1de6a4d98530c6aa570e71b40f7c4442241ed5fa22`; reproducibly
regenerated 416,576-byte HRIR data is
`11200599389c4e2c60144ab7636e4c0ba130a40c7f12acb77ae88be844456551`. No new
runtime dependency is introduced.

A per-world owned BVH contains sliced wall solids, shared canonical glass bounds
and collision-based static scenery proxies. Material-bearing walls/panes are
deduplicated against generic collision geometry. The final review found and
fixed a solid-pane duplicate; its regression proves one pane transmits 0.2,
rather than applying its coefficient twice. Actual oriented door poses
invalidate paths; solid scenery uses its existing proxy, not triangle-exact
model geometry. Movable physics props are not promoted into stale static
blockers. This is a bounded acoustic approximation, without diffraction or wave
simulation.

Each voice owns one cached path. Eight queries run per 20-ms tick, round-robin,
with listener displacement above 5 cm, emitter changes, door revision and new
world ownership invalidating results. Each query is capped at 256 BVH nodes and
256 door tests; the pre-existing level door limit is 24. Dense 48-voice refresh
can therefore take up to approximately 120 ms. Exhaustion conservatively muffles
and records the event without growing queued work. Head orientation/HRIR targets
update at 50 Hz. Gain/filter and binaural transitions are smoothed; measured FIR
coefficients and post-effect master gain use 576-frame/12-ms linear transitions.

Low/mid/high material absorption and transmission use existing material IDs.
Carpet/acoustic defaults damp more than hard tile; authored overrides, room
wetness/decay, dimensions/shape, floor patches, openings and live door aperture
shape a bounded Sabine response, early taps and four stereo comb delays. Room
response changes smoothly. One shared five-line effect owns 327,680 delay bytes;
master mute/pause follows the effect and silences its tails. World epochs clear
old delay samples. This is listener-room acoustics, not a traced propagation
network between every pair of rooms. Wall-face materials are averaged and
non-wall scenery proxies use generic coefficients.

The existing editor forms add Acoustic material and Room ceiling/acoustics
fields; ordinary Preview/Apply/Undo/save/reopen and typed validation own the
changes. Invalid coefficients, duplicates and missing assets are refused.
Nondefault authored acoustics require `material-acoustics-v1`; old maps retain
defaults. Editable compile/export/import is byte-exact. Audio-only authoring
preserves prepared geometry/light bake keys and render material slots; a focused
regression protects both. Existing Luau sound/playing and moving-entity anchors
use this mixer. A bounded 64-entry SoundEvent start-observation drain and
public, device-independent AcousticPath form clean seams for later shared
perception. No AI, new editor phase or Backrooms work is included.

**Audit findings and corrections.** Initial scalar FIR mixing, and a first
Vec4.mul_add prototype, failed the real-time budget. Portable glam lowered that
fused operation to scalar fallbacks without FMA. A quiet 48-voice prototype used
268.73% median mixer wall/audio duration and was rejected. Explicit Vec4
multiply/add retains every tap and passes scalar equivalence within 1e-5. The
isolated locked-glam probe measured 0.2720045 -> 0.0038513 seconds for the same
work/result, without target-cpu/native flags. Final native figures below, not
that microbenchmark, establish the callback result. Finite sounds also now keep
the complete FIR/IIR tail before relinquishing their mixer slot; a tiny mock
regression protects end-of-block impulses and bounded reclamation.

The first dense stimulus used gain 0.005, falling below Kira's -60 dB silence
floor after distance/category gain. Its occupied-slot numbers (including the
earlier 4.86% 48-HRTF figure) are withdrawn as audible stress evidence. The
final fixture uses gain 0.5 solely for validation, checks every fluorescent
voice above a 0.001 gain floor, rejects zero native PCM and retains four dense
WAVs. Production cue gains are unchanged. This audit-only correction preserves
the exact game/editor/compiler binary hashes
(`audit-only-correction-proof.json`). The final performance set combines nine
unchanged-player Pool receipts from `matched-pane-final/pool-*` with nine new
audible dense receipts from `matched-audible-dense/`;
`matched-audible-final/analysis.json` links each source. The early failed
fused-FIR timing is a complexity diagnostic, not a matched before/after
audible-quality measurement.

The first final-analysis predicate incorrectly included MainMenu/loading and
required a positive cumulative peak before the first asynchronous audio
callback. The corrected predicate uses ready-to-end Playing snapshots, requires
all 48 voices and every gain above 0.001 throughout, requires zero mixer errors
and positive output by the end. It accepts all nine audible dense controls and
still rejects all nine earlier silent controls under the same predicate. The
independent guarded device audit records 95,438-101,397 nonzero native PCM
frames in each of its 32 cycles; audible dense pause and motion WAVs corroborate
actual signal. This does not establish subjective audibility/localization.
`guard-reconciliation.json` records the positive/negative controls; the
byte-identical failed `native-audible-workflow/initial-results.json` and failed
log remain preserved. Its final `results.json` explicitly records initial
failure and the resolved analysis, rather than rewriting it as an original
success.

The callback has no new geometry/decoder/I/O/heap preparation or waiting lock.
The pre-existing Kira resource gate remains a non-waiting atomic try-acquire; a
contested command boundary defers publication while existing voices render. SDL
serializes its own stream callback. Fixed two-entry per-voice and four-entry
output SPSC controls bound communication; the consumer drains them each block.
Rapid unlimited game updates may count full control queues, dropping redundant
updates rather than creating a backlog; measured mute response remains bounded.
The existing admission policy is retained: 48 mixer slots, source replacement,
bounded loop retry and refused excess one-shots, without new priority stealing
or virtualization. Clear retires pending controls, emitters, scene and events.

**Matched baseline/final performance.** Windows 10 Home 10.0.19045, Ryzen 9
5900HX (8 cores/16 threads), 16,537,948,160 RAM bytes, RTX 3070 Laptop/DX12;
portable locked optimized release/all features. The baseline player is
`19a4a3a22718e5f340919e8281e2c9aae9fbab4e260244493ab75e258ecdf0ba` at the base
commit; the exact final player is
`f2f9d86dda736e3f4d5ccf88d5a0a56164844ee117c60ffbdae3911015244e74`. Three
balanced-order 15-second ready runs per map/mode exclude 120 warmup frames. All
use High/filtering/Full lightmaps/reflections/bloom, 1280x720 physical, real
delta, VSync/frame limit off and verified Immediate presentation. Throughput
exceeds the 165-Hz display. No captures, detailed CPU/GPU trace or concurrent
build is included in these primary runs. Pool uses fixed eye/listener (8,1.6,9),
yaw 0/pitch -32, with three ambient voices. Dense uses (5.5,1.6,5), yaw/pitch 0
and 48 comparable fluorescent sources behind the same door.
Master/effects/ambience are 0.8/1/0.6; mute is off. The logical default is Steam
Streaming Speakers, WASAPI, 48 kHz/stereo, 480 frames, as identified by the new
native logs and contemporaneous device enumeration; the unchanged baseline uses
SDL's same logical-default API but did not log its resolved endpoint name.
Medians of independent runs follow; raw ranges remain in the analysis receipt.
Mixer percentages use native first-ready/end counters (including audio onset;
frame FPS alone excludes 120 warmup frames) and divide elapsed mixer wall time
by rendered audio duration, not by total machine capacity, and exclude SDL
transport/driver work.

| Map | Mode | FPS | Loop p99 ms | Update mean ms | Update p99 ms | Mixer wall/audio |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| pool | base | 277.22 | 7.196 | 0.2831 | 0.510 | 0.471% |
| pool | speakers | 275.07 | 7.190 | 0.2882 | 0.515 | 3.136% |
| pool | headphones | 277.37 | 7.213 | 0.2890 | 0.540 | 4.028% |
| audio_dense | base | 912.63 | 1.639 | 0.0468 | 0.121 | 8.087% |
| audio_dense | speakers | 901.78 | 1.705 | 0.0532 | 0.141 | 20.861% |
| audio_dense | headphones | 904.80 | 1.663 | 0.0574 | 0.189 | 34.239% |

 Measured throughput change: pool: speakers -0.78% FPS, headphones +0.05% FPS;
audio_dense: speakers -1.19% FPS, headphones -0.86% FPS. These small changes
retain the added full-quality processing; raw tails/ranges are reported rather
than labelled an audio optimization.

| Map/mode | Request-to-ready median ms | OS peak working set MiB | Sampled peak private MiB | Whole process CPU seconds |
| --- | ---: | ---: | ---: | ---: |
| pool/base | 8887.02 | 565.65 | 945.77 | 23.656 |
| pool/speakers | 8841.99 | 578.77 | 960.45 | 23.891 |
| pool/headphones | 8902.76 | 578.75 | 959.06 | 23.281 |
| audio_dense/base | 7850.99 | 343.29 | 469.41 | 22.984 |
| audio_dense/speakers | 7892.86 | 343.47 | 480.29 | 25.312 |
| audio_dense/headphones | 7824.57 | 343.81 | 475.93 | 26.984 |

These process figures include startup, renderer/driver allocations and worker
threads. Private bytes are 250-ms samples, not allocation-call counts or total
GPU memory. Initial loading/first-ready cost remains separate from the warmed
frame table and does not establish a fix for the prior cold-input stall.

**Native device, latency and resource checks.** Exact-final default-device game
runs and explicit process-only Realtek output are exercised without changing
host mixer/default/THX settings. Realtek reports WASAPI, 48 kHz/stereo and
512-frame (10.67-ms) buffers. The opt-in allocator/lifetime tool ran 32 cycles
in 82.87 seconds: office/tile contrast, Speakers/Headphones, 3/48 voices, moving
source/listener orientation, rapid source restarts, overflow admission, mute,
pause and clear. All dense admissions reach 48; overflow is refused.

| Device audit mode/voices | Mixer wall/audio median | Main-thread acoustic update mean, median across cycles (us) |
| --- | ---: | ---: |
| Speakers-3 | 2.606% | 1.054 |
| Headphones-3 | 3.491% | 3.717 |
| Speakers-48 | 17.077% | 4.280 |
| Headphones-48 | 28.437% | 23.290 |

A separate 48-audible-voice listener translation/rotation diagnostic measures
35.421% mixer wall/audio, maximum block 5.3216 ms and 3,968 path queries against
a 4,224 bound over 10.501 seconds. Native PCM is finite/nonzero, all voices
exceed the gain floor and no capture loss occurs during motion. This
capture/trace run makes no FPS claim (`native-dense-motion-final/`).

Maximum measured mix block in the standalone audit is 4.8600 ms. Callback Rust
allocations/frees, capture loss, mixer errors, query-budget exhaustion and full
parameter queues are all zero in this quiet device audit. Six post-mixer trials
measure cue request to first PCM signal at 1.9989-6.0022 ms; mute to a confirmed
10-ms exact-silence witness is 28.1084-32.0689 ms, with requested 1-ms polling.
These are native post-mixer observations, excluding physical transport/DAC and
perception.

A same-cue, same-position open-room baseline/final control exercises ordinary
authored PlaySound/StopSound and pause. These are PCM frame-index proxies
anchored immediately before dispatch at the preceding callback counter. Zero
means the first sample in the next recorded block, not zero wall latency.
Resolution/phase uncertainty is one 10-ms default-device block plus dispatch;
physical transport is excluded.

| Mode | Cue-start frame proxy ms (three trials) | Single-cue pause plus 10-ms silence ms | Audible 48-voice pause plus 10-ms silence ms |
| --- | ---: | ---: | ---: |
| base | 0.000-0.000 | 10.000 | 10.000 |
| speakers | 0.000-0.000 | 10.000 | 31.979 |
| headphones | 10.083-10.083 | 12.896 | 31.979 |

The audible dense pause witness increases from 10.000 to 31.979 ms.
Approximately 22 ms extra is consistent with the new 12-ms post-effect fade plus
one 10-ms output-control callback phase. This measured cost is retained to fade
reverb cleanly; it is not called immediate or treated as a listening result. The
direct final-device wall-time witness above remains the more precise final
measurement.

The shared 66-clip decode cache is unchanged at 43,409,440 bytes (41.398 MiB),
retained once across map swaps. Native audit live Rust bytes are 44,593,781
after initialization, with peak 66,645,489. After explicit unload all
voice/clip/decode/scene counters are zero. Live bytes then measure 442,996
including retained audit JSON/scratch; dropping that harness storage leaves
6,238 bytes, versus 270 before audio initialization. This small residual is
reported, not asserted to be audio-owned or zero. Rust accounting excludes C
SDL/driver allocations. OS peak working set is 66,301,952 bytes; process/private
counters and all per-cycle live samples are preserved. This finite stress run
shows bounded ownership and cleanup; it is not a multi-hour listening or
driver-leak proof.

The final player passes 19 checkpoints for closed/opening/open/closed-again leaf
and solid wall, one correctly deduplicated glass pane, listener front/back/
elevation, mute/master-zero, pause/resume, Low/High reload, restored stereo and
fluorescent power-off/restore. Captured PCM is finite, peak 0.038504; central
mute/master-zero/pause windows are exactly zero and resume is nonzero. Initial
startup tap loss is 228,608 frames while the main-thread capture consumer waits
for loading; it does not grow across the stable checks. This optional tap is not
a complete startup/underrun witness. Existing faint fluorescent, quieter HVAC
and subtle pool cue files/gains remain unchanged, with ordinary power envelopes
and master/category controls.

Five lab and four water-map installs plus three Low/High cycles, cancellation
and a deliberately corrupted disposable installed-package failure retain the
correct current world. Warm ready transitions span 349.82-716.81 ms; initial lab
startup is 7775.43 ms and is not hidden in ready FPS. Lab checkpoints own four
voices/6 acoustic surfaces; water checkpoints own two/385. All pending counts
remain zero, the shared decode cache stays constant, old authored sources
disappear and shutdown clears world voices/scenes. The 250-ms OS samples include
graphics caches and are not Rust allocation counts. The original lifecycle
harness confused the two maps' valid pool_lapping cue names; identity/position
analysis corrects that assertion with the same raw run. A missing named output
logs the established silent fallback and the player continues with disabled
audio and zero voices. No actual unplug/ replug or dedicated physical headphone
endpoint is claimed.

**Validation, evidence and remaining acceptance.** Exact final format, strict
workspace/all-target/all-feature Clippy, **336 Rust tests**, locked optimized
workspace/all-feature release, assets/texture/prop/audio checks and whitespace
pass in `quality-audible-final/results.json`. The tests use tiny fixtures, with
no shipped-map bake, native device or performance run added to automatic tests.
The prior broader Python 17 failures/errors and 16 skips remain separate and
were not rerun. Authored acoustic and legacy dense projects each check, force
export, validate, verify current and recover their two editable files byte-for-
byte under the exact final compiler (`package-pane-final/`).

Unique raw evidence is preserved outside target at `C:/Users/Connor
Dawkins/Documents/Codex/2026-10-25/task-6/`: final quality/package receipts,
`native-audit-audible-realtek/`, `native-functional-pane-realtek/`,
`native-lifecycle-pane/`, `native-missing-device-pane/`,
`matched-audible-final/`, `matched-audible-dense/`,
`native-latency-control-audible/`, `native-dense-motion-final/`, final
binaries/licenses/hashes, source proof and custody/cleanup receipts. Rejected
scalar/fused-SIMD runs, pre-pane binaries/measurements, harness failures, source
archive, regenerated HRIR and all local WAV/screenshots are retained with their
distinct provenance. No media attachment, CI wait, release, version bump, main
merge, force push, host setting change or iGPU pass is performed.

Subjective ambience balance, crackle-free motion, useful headphone front/back/
elevation and audible office/pool contrast still require actual listening;
measured spectra, impulses, state and generated/native PCM do not establish it.
Prior ambient listening remains pending. macOS/Linux native devices are not
available in the selected Windows executor, and the integrated iGPU pass remains
later. Individual HRTF fit, actual host/DAC latency and physical hotplug remain
unverified. The accepted water residual, cold input stall, CRT review, broader
Python issues and final version gate remain distinct. Commit/push/remote hash,
clean-tree verification, preserved-deliverable hashes and idle-only Cargo clean
are recorded in the final local ownership handoff.

## 2026-10-26 — shared AI foundation, six behaviours and existing-content migration

**Status: shared AI implementation, current-content rebuild, native migration and
matched performance validation completed. Final Git/custody receipts are local.**

This staging increment implements the approved shared AI scope on the
remote-verified audio base `1863c947e5e2d46b0e7085ad852b83c857dd33ad`.
The sole writer retained custody throughout. Existing A* clearance, door
negotiation, replanning, avoidance and stuck retries are reused. The original
0.20-second staggered perception, eight path queries per tick and 8,192
expansions per query remain bounded; this does not introduce another mover or
an editor-only AI interpreter.

**Runtime and ownership.** `engine/ai/authoring.rs` owns the serde contracts,
`controller.rs` arbitration and operation identities, `tasks.rs` bounded
behaviour execution and `senses.rs` mapped observation/alert state.
`entities/ai_runtime.rs` adapts intent into existing world actions; native
carry/render/physics remain responsible for the actual mesh, socket and body.
Baseline idle/wander/prey/predator/follower and authored sequences participate
in the same motion arbitration. Priority 0–200 selects a task; caught, physical
ragdoll, light hold and sequences have explicit interruption semantics.
Task preemption and explicit pause retain continuation and simulation clocks;
cancellation, caught, despawn and world replacement invalidate their operation
and release reservations/cargo. Ragdoll restoration creates fresh authored AI.
Baseline resumption restores the old state, destination and custom speed, rather
than merely returning to an idle label. Queued cancellation cannot erase the
active owner's goal. A world epoch, generation-bearing EntityHandle and monotonic
operation serial reject delayed physical effects after cancellation or reload.

The original lost-target predator bug read the moving live target during
advance. It now travels to observed last-known state, searches for a finite
interval and reacquires through actual perception. Actor sight, catches, Luau
visibility and shared tasks use static/door/live-solid LOS. An object query
excludes the endpoint's own solid body, while a player/entity query cannot
mistake an intervening live crate for the target body.

**Six shared behaviours.** Fetch searches observed loose items by exact id or
model type, reserves the chosen generation and follows sampled last-known item
state. It samples eight objects per 0.20-second observation, explores expanding
rings when the item is hidden and retries unreachable search points with a
bounded cooldown. A player-held, stolen, removed or unavailable item cannot be
claimed. Actual pickup waits for the posed hand to move and settle against the
original item. The existing render/physics key is transferred into the socket
assembly; no replacement mesh is spawned. Cancellation and interruption restore
the original PhysicsDef, bounds, scale and mass at the current carried pose.
Delivery navigates to an explicit stance/yaw, waits for physical putdown contact
and actual destination proximity, then dispatches the mapped interaction.
Raised shelves require the original body to be released at the contact and
supported by the actual surface. Failed contact, lost cargo and root timeout
report inspectable results and recover ownership.

Investigation/search accepts typed movement/door/interact/spawn/impact/sound/
alert noise, empty-any or selected category filters and an individual NPC
radius. Sight filters player, other entities, exact object ids and model types
with authored range/FOV and geometry LOS. Observations map to investigation,
the same typed TaskRequest or a bounded alert. Follow retains only actually
observed contact, stops live tracking behind geometry, times out memory fairly
and uses existing navigation/stuck recovery after route or obstacle changes.
Use supports doors, chairs, switches, shelves and generic interactable points,
with distinct feet/contact positions, reach, optional yaw and posed socket,
clip/contact time and hold time. Routines compose these same behaviours and
resume their active child after urgent work, sequence ownership or pause.
Alerts have at most 32 records, four propagation hops, six-second TTL, 16 relay
positions and 256 visited generations; recipients share normal task priorities
and perception semantics, preventing rebroadcast loops.

**Bounded authoring and migration.** Each NPC has at most eight tasks and 16
sense rules. Task trees are bounded to 256 nodes, depth three and 32 entries
per list; world task object references are capped at 512. Object observation
uses a rotating window and bounded pinned references at perception cadence;
baseline-only scenes skip that cache. Sight work rotates eight candidates per
rule. Native pending uses are capped at 64. Luau accepts plain tables or JSON
through the same validated schema, with 512-node/depth-12/32-entry/16-KiB input
bounds and invocation-atomic action publication. Cycles, metatables, sparse
arrays, functions and non-data values are refused. `api.ai_task`, `api.ai_pause`
and `api.ai_senses`, matching serialized actions and `ai.tasks`/`ai.senses`, plus
actor owner/task/phase/result snapshots are documented in the canonical authoring
guide, scripting guide and type declarations. This is the stable substrate for
the next editor phase; no Simple/Advanced, drag-resize, template or visual-tree
UI was implemented here.

All four Beam modules now configure the shared patrol/search task while Luau
retains quest gates, grace periods, refuge/light rules, authored 0.8-second
patrol decisions, catch/death/reset timing, objectives and sound cues. One Guest
configures the shared follower and explicit pauses around survey/doorway/room
checks, retaining six arrivals, the 2.5-second room pause and proof of a physically
closed, unobstructed locked door. Current Home's caretaker baseline and menu's authored parcel relay continue
through existing primitives. The default-menu check exposed its stale package
requirements. Connor's project-wide pre-release policy is now explicit: until
full release, update owned content to current formats and add no compatibility
layers. The provisional old-script admission exception was removed. Script-
bearing and configured-AI packages require `shared-ai-v1` under strict reader
validation, with a tiny semantic-admission regression. The bundled menu was
rebuilt and verified under the exact final compiler, with its original unique
bytes preserved outside the repository. All 16 authored projects (32 internal
levels) were rebuilt and verified; six playable distributions contain 19
internal levels. The menu, nine developer/review projects, 33 fixtures, two frozen
historical sources and 462 historical validation/recovery archives are separately
inventoried. Only the menu and two active One Guest export targets are refreshed;
historical raw evidence remains immutable. The policy is recorded in the package
and authoring guides and carried into the next handoff.

The audio interface emits bounded, deduplicated typed gameplay stimuli from
authored starts, footsteps and impacts. It does not change DSP, voice limits,
cue assets/gains or the accepted audio device/listening residuals. Physical
local correction runs only when normal A* has no usable route, probes the same
body/floor/ceiling/door/actor collision and never teleports. Native acceptance
caught both an overly eager correction that competed with a valid route and a
released parcel trapping the returning baseline. Normal A* now wins; separating
escape from an externally introduced dynamic overlap is permitted while motion
deeper into a solid remains blocked.

**Validation evidence.** Final validation and measurements are recorded below;
raw unsuccessful candidates and corrected fixture/audit attempts remain separate
under `C:/Users/Connor Dawkins/Documents/Codex/2026-10-25/task-8/`. Native captures
are functional evidence and excluded from throughput measurements. Dedicated
fixtures are explicit developer tooling, never automatic map bakes or new modes.


Native acceptance also exposed delivery self-occlusion after the original body
was restored. Contact rays now exclude the intended reserved body while retaining
static/door/other-solid tests. A native raised-shelf run then found the carried
render origin could still be sampled at the collider edge during release. The
adapter now consumes the existing solver shape-centre path, and release publishes
the restored physical centre before the next AI tick. This preserves script and
render transforms and adds no world scan. Tiny tests protect strict admission,
own-body contact versus a separate blocker and rotated physical observations.
Final-player floor and raised-shelf native acceptance both pass with the original
render/physics identity and actual raised support. Earlier failed candidates,
fixture corrections and aborted/superseded helper jobs remain raw evidence.


The exact candidate-21 source passes strict all-target/all-feature Clippy,
**351 Rust tests** and locked optimized workspace/all-feature release. Its
source hashes are unchanged through validation/build. The final player passes
**21 native shared-AI scenarios / 416 checks**, plus all six baseline navigation
cases: layered floors, clearance, doors, movable obstruction, meeting actors
and unreachable recovery. These are functional/capture runs with current-content
compilation active, and supply no throughput claim. Floor/table delivery,
obscured/moved/stolen/held/removed items, competing owners, actual socket
contact, routines/priority/sequence/pause, ragdoll, light-held cargo, typed
perception, bounded alerts, follow obstruction, caught cancellation and inter-floor
original-item delivery are exercised through the real native player.

The corrected three-cycle unload/reload run reaches epochs 2, 4, 6 and 8.
Each returned fixture has eight AI agents, two authored bodies, zero attachments,
zero tasks and zero reservations. Retired-world weak witnesses report zero
prepared-build, prepared-record and transport owners. Live process heap bytes
are 153,061,940 initially, then 160,915,455 / 163,636,510 / 163,644,308 on the
three returns; source image bytes stay 36,831,232 and active prepared bytes
7,046,221. These heap counters include native/driver work, exclude VirtualAlloc,
and include benchmark/model-cache retention; they are not AI allocation-call
counts or a multi-hour leak claim. OS process counters and bounded frame-record
capacity remain in the raw receipts. The original reload run completed its
cycles but hung because its harness anchored Quit to a checkpoint after a
capture; normal window close was recorded, the harness was corrected and only
reload was rerun. No engine fix or repeated passing AI case was required.

**Completed current-content and story gates.** All 16 project distributions
(32 authored members, including the internal menu) exported Full and Static (`off`),
passed package validation and passed current-source verification with the pinned
candidate-21 compiler. Authored project sources remained byte-identical throughout
the rebuild. Three compiler lighting advisories remain recorded: no valid
irradiance probe at 1.500-m spacing in Beam room 1 and the unnamed authoring
project rooms 6 and 10. They do not fail validation/current verification; no
lighting or geometry was changed to suppress them. Exact warnings and variants
are in `current-content/warnings-summary.json`. Six playable distributions
contain 19 internal levels; the menu
and nine developer/review projects are counted separately. Native current-reader
discovery and real presentation/simulation pass for all 31 non-menu members.
This is startup/load smoke for unchanged content, not proof of every unchanged
quest. Full coverage retains the first 13 passing checkpoints and checks the
remaining 18 in fresh processes. A separate Static probe passed seven members
before correctly refusing Closing Time, whose authored circuits require Full;
there is no blanket claim that every level supports Static.
The initial Full campaign hit the existing conservative 2-GiB overlap limit on
Arcade to Lantern Hollow (877,234,056 displayed plus 1,382,595,604 incoming
charged bytes). The old world remained intact and the failed campaign was
closed normally. This is an actual transition-budget boundary, not a format or
AI error; no budget was raised and no package or quality was downgraded. The
existing explicit Enter unload-and-retry route is separately exercised in the
native player, preserving Arcade at refusal, then unloading and successfully
loading/simulating Lantern Hollow Full. That initial
failure and its provenance are retained separately from the completed member
receipts. The separate bundled-menu relay run passes original parcel/body/mesh
transfer through both porter and guest, with no duplicate body while carried
and actual release between collectors. All seven authored vignettes remain in
the source; this one relay does not establish full listening/visual coverage of
every vignette.

All six quiet ordinary-control migration runs pass: Beam Home, Pool, Office,
Arcade, Beam light/catch mechanics and One Guest Too Many. They observe shared
tasks while retaining authored completion and narrative controls; One Guest
observes all six guests and an actually closed locked leaf. The earlier Pool
budget failure under concurrent compilation is retained separately and does not
relax the production budget. Arcade's first fixed-camera route returned every
item but aimed once while the shadow remained occluded and missed the seal.
Only Arcade was rerun: identical movement/item inputs, three extra camera-only
reacquisitions and five seconds extra waiting pass the unchanged two-second
seal, actual unlocked exit and win. The five passing routes were retained;
existing snapshots additionally verify freeze/grace/pursuit/reacquisition/catch/
reset and the named unobstructed closed locked store leaf with the original
follower physically inside. The final production Luau guard passes both
projects and all 11 resource/exception refusal cases.
Asset gates pass: 629 assets, 389 placeable entries, nine themes, zero validation
warnings, plus audio, props, textures and clip boundaries. The 27 targeted Python
contract checks also passed; the older broader Python failures/skips were not
rerun or reclassified.

**Matched uncapped measurements.** Three interleaved baseline/candidate pairs of
twenty-ready-second runs use the same navigation archive, timed nine-NPC route changes,
camera, settings and 120 warmup frames. Both use real delta time, physical
1280x720, actual Immediate presentation, VSync off and no limiter; observed FPS
exceeds the 165-Hz panel rate. High texture quality/filtering and the same Low
lighting setting are used in both runs (lightmaps/reflections off); this is a
navigation/runtime comparison, not a new water optical measurement. No captures,
builds or content bakes run during timing. Table values are medians of three
runs; worst columns are medians of each run's worst frame, not pooled maxima.

| Metric | Baseline | Shared AI | Change |
| --- | ---: | ---: | ---: |
| Average FPS (FPS) | 482.018 | 475.335 | -1.4% |
| Loop p95 (ms) | 2.890 | 2.974 | +2.9% |
| Loop p99 (ms) | 4.127 | 4.169 | +1.0% |
| Loop worst (ms) | 19.777 | 19.965 | +1.0% |
| Update mean (ms) | 0.580 | 0.612 | +5.6% |
| Update p99 (ms) | 1.028 | 1.161 | +12.9% |
| Update worst (ms) | 18.203 | 18.388 | +1.0% |
| Whole-process CPU (s) | 23.547 | 24.375 | +3.5% |
| Sampled peak private (MiB) | 470.992 | 469.547 | -0.3% |
| OS peak working set (MiB) | 338.637 | 337.324 | -0.4% |

Loop p99 ranges are 3.728-4.724 ms
baseline and 3.680-4.523 ms final;
update p99 ranges are 0.998-1.174
and 1.136-1.695 ms. Frame counters
exclude startup; OS CPU/private/working-set counters cover the complete process.
Private bytes are sampled every 250 ms and are not Rust allocation-call counts
or GPU VRAM. Thermal/session variance is not removed by three samples.

An earlier time-separated comparison measured 551.6 versus 467.8 FPS and loop
p99 2.864 versus 4.260 ms. That prompted the same-session interleaved control
reported above. The interleaved median throughput difference is -1.4%, with
update mean +0.032 ms and update p99 +0.133 ms. Frame-tail ranges overlap; the
additional CPU cost is retained explicitly and zero regression is not claimed.
Both earlier groups remain in `final-metrics-initial-session-comparison.json`,
`baseline-navigation/` and `final-navigation/`; no host settings or GPU clocks
were inspected or changed.

The separate absolute new-task workload runs at 414.4 FPS,
loop p99 4.183 ms (worst 8.135),
update mean/p99 0.844/1.424 ms,
whole-process CPU 25.719 s, sampled peak private
463.7 MiB and OS peak working set
332.4 MiB. This is not a baseline
feature comparison. Its existing shared-task fixture and acceptance actions
provide the absolute workload; the 21 functional cases remain separate proof of
physical behaviour and ownership. All workload actions and normal shutdown are
verified. The task workload retains a 19.5-second lifetime checkpoint inside the
measurement; its serialization cost can affect the worst frame. The matched
navigation comparison has no such checkpoints.

**Provenance and handoff.** Player SHA-256:
`a870b627ff177f41551f8cb902b2a87307812aa8caeacfe5c6528904d10d5da6`; compiler:
`8d486f5b0a42d5f8c5d357789e3ed8ac2bd345cd2cebb3ef58185364272bd82e`. All gates and rebuilt content use those pinned
executables. Source review subsequently restored the original UTF-8 punctuation
in exactly two pre-existing documentation-comment lines in package/world.rs;
no runtime tokens changed. Exact as-built bytes, the two-line delta and hashes
of every other unchanged Rust/Cargo input are retained in
`native-build-21/final-source-lineage.json`. Formatting and whitespace checks
pass after that comment-only restoration.

Raw receipts live outside target in task-8: `current-content/` (all packages,
source proofs and original active-target backups), `native-current-content-final/`,
`native-current-content/` (original Full transition-budget refusal),
`native-current-content-full-resume/`, `native-current-content-static/`,
`migration-native21/`, `migration-native21-arcade-adjusted/`,
`migration-native21-final-results.json`, `native-full-budget-retry/`,
`native-menu21/`, `native-final21-ai-matrix.json`,
`native-final21-lifetime.json`, `native-final21-navigation/`,
`native-build-21/`, `assets-ai-final21/`, `luau-release-ai21.json`,
`baseline-navigation/`, `final-navigation/`, `paired-session-navigation/`,
`paired-metrics.json`, `final-task-stress/` and
`final-metrics.json`. Unique old menu/export bytes, failed candidates and harness
attempts remain preserved. Final source/binaries/symbols/packages are hash-checked
before idle-only Cargo cleanup. Exact staging commit, verified remote hash,
clean-tree and cleanup/custody receipts are recorded in `ownership-handoff.json`.

Water retains its accepted optical p99 cost (4.211 versus 2.988 ms). Audio
listening, headphone fit, physical hotplug and platform coverage remain
unverified; dense pause still adds approximately 22 ms. The cold approximately
2.4-second input stall and CRT reflection review remain separate. No main merge,
force push, release/version bump, ordinary CI wait,
host-settings change, GPU-clock diagnosis, iGPU pass or Mac switch occurs here.
No new pre-release compatibility layer is admitted: later phases must rebuild
affected owned content. The next approved task is the editor using Astra High,
then Backrooms, integrated polish/one iGPU pass and the final version/package gate.

## Editor phase — native authoring, 2026-10-26

The editor queue phase is implemented on staging after shared-AI commit
`47c2938f48c68a709d13ff3daad3a3c56f8db5fb`. Simple is the default action-oriented
workspace; Advanced retains the existing inspector. Both use one document,
selection, script buffers and undo history. New Room has preview/cancel and
position, footprint and storey controls. Horizontal labels are Room1/Room2;
stable IDs and references are unchanged. A room creates floor/ceiling geometry;
walls, openings and stairs remain explicit authoring choices.

Yellow viewport edge handles and matching numerical fields expand a room in a
chosen direction, keeping its opposite edge fixed. Snapping/live dimensions,
one-step history and Escape/focus-loss cancellation are integrated. Perimeter
walls, doors, fixtures and wall decals move together; side-wall openings retain
world positions. Shared boundaries, unsupported shrinks, spanning walls and
uncertain elevated fixed-prop attachments are refused rather than guessed.
This is conservative rectangular-room manipulation, not arbitrary topology
editing. Ceiling geometry remains present; an editor-only material flag hides
the top view while showing the textured underside from below. Player geometry,
collision and lighting are retained.

Event forms create readable, uniquely named editable Luau modules for start,
interaction and volume entry/exit. Message, door/sound control, investigation,
fetch/delivery, follow, environment use and keyed pause/cancel use supported
shared APIs. Blank/custom source and module/line diagnostics remain available.
Regeneration creates a new file and never overwrites existing edits. Contact
positions and facing are explicit physical constraints, not a reachability
promise. Native testing caught a missing delivery facing hint; final generated
fetch scripts include it and complete actual pickup, transport, release and
delivery interaction.

The behavior panel accepts real drag/drop ordered blocks and sibling reordering,
nested routines, waits, movement, investigation/search, fetch, use, follow and
patrol. Hearing supports Any/selected categories and radius. Sight supports
player/entity/instance/type filters, range/FOV and the shared LOS checks.
Responses run ordered routines, investigate or send bounded alerts. Existing
typed AI contracts own execution, task priorities, failure termination,
reservations, cancellation and navigation/perception budgets. Empty or excessive
plans and missing references are refused. Visual metadata preserves the plan
when a new Luau copy takes ownership. Applying visual edits while Luau owns
execution requires an explicit choice; the old file and edits remain. Dirty
drafts survive panel/mode changes, refuse accidental NPC replacement and require
Reload after a scene revision. Inspect last Play reads bounded, paged real-player
snapshots with actor state, task keys, pause/active flags, phase/step and result.

**Settled-code gates.** Strict workspace/all-target/all-feature Clippy passes;
`cargo test --workspace -- --test-threads=1` passes **355 tests**; locked release
workspace/all-feature build passes. Four small source-only editor regressions
protect atomic edge/opening/decals/history, connected-boundary constraints,
visual/Luau ownership and transactional template creation; no bake/GPU enters
ordinary tests. The first parallel suite passed; a later parallel run under
concurrent load hit the existing automatic-script-handler timing count (264
versus 266). Subsequent full serial runs pass; no test was weakened. Asset
validation, audio/props/textures checks and clip boundaries pass (629 assets,
389 placeable entries, nine themes). Texture checks retain 100 existing warnings.
The production Luau boundary gate passes both authored projects and all 11
refusal cases. Nine targeted Python geometry checks pass. The separate 16-test
tool suite has 14 passes, one Windows unsupported-SIGINT error and one same-name
capture CLI failure (expected 11 REGRESSION markers, got zero); production
Python was not changed and these are not reported as green.

**Native acceptance.** Real editor input journeys cover both modes, repeated
switches, connected and stacked rooms, directional drag/numerical resize,
undo/redo, a separately verified live-drag Escape cancellation, stair clearance
refusal then preview/cancel/apply, actual themed asset placement at exact
coordinates, textured ceiling underside, save/reopen, build progress/cancel,
repeat build, export and recovery. Recovered canonical manifest, level and all
referenced scripts are byte-identical. Existing export behavior includes
referenced modules; inactive regenerated modules remain in the original project.
Native behavior journeys cover actual block drag/reorder, invalid empty loop,
custom edits and syntax location, explicit execution ownership, unique
regeneration, invalid template key, referenced-object deletion refusal, stale
draft refusal/reload and interrupted forms. Readability was inspected at normal
size and at 960x544/1920x1080. The 1,192-object Lantern Hollow inspection retains
source bytes through search/scroll/modes/resizing. Its measured input-handler
median is 1.288 ms, maximum 246.785 ms, excluding async waits; initial preview
wait is 14.27 s. These are responsiveness observations, not a throughput or
zero-stall claim.

Physical player acceptance demonstrates a visual routine moving an NPC, generated
Luau loop execution, exact pause-clock stability and cancellation without restart;
generated fetch physically carries the original parcel, releases its reservation,
finishes `fetch:complete` and actuates the delivery light. Selected hearing rejects
the wrong category, Any hearing responds, and object-type sight refuses an
occluded object then responds when visible. Follow/escort responds to changing
player destination. Alerts reach configured receivers once and expire without
feedback; a generated Message opens and dismisses the actual UI. Failures from
blocked routes and the earlier bad contact remain inspectable raw evidence.
Full prepared loading and reflection loading also pass. Final editor Build+Play
reaches the real player and its post-Stop inspector displays live task state.
Earlier fixed-duration Play attempts stopped before ready under load; they do
not establish startup coverage. Candidate 5 replays fetch, loop,
selected-perception and Any-hearing; a **76-assertion evidence audit** checks
accepted runs, physical outcomes, no script/audio errors, source preservation
and exact compiled-source hashes. It is not a count of new Rust tests or a
repeat of the prior 21-scenario shared-AI campaign.

**Acceptance correction and current content.** The final terminology sweep
changes 76 display strings in 20 editor files: horizontal selections, prompts
and errors say Room; physical material/region controls say Surface; Floor
denotes a vertical storey/elevation. Stable IDs, serialized keys, references
and all non-string Rust tokens are unchanged. Final-binary native checks cover
Room1 selection, the invalid rotation refusal without source mutation, upper
Floor creation, lower/upper Room stair selectors, New Room elevation, repeated
mode switches and save. The four corresponding assertions pass.

The first editor handoff left a compiler-fingerprint freshness mismatch in
all prior packages. That acceptance gap is now closed. All **16 project
archives / 32 members** have been rebuilt with the final compiler through the
existing supported pipeline, using eight workers and sequential project jobs.
All members pass strict current readers and `verify --require-current`, with
integrity valid, no differences and `bake_required: false`. The separately
bundled menu and both active One Guest outputs were preserved before replacement,
then rebuilt, verified, published and verified again. The existing planner
reuses genuinely current exported preparation for the standalone outputs;
stale preparation was not exempted. The final audit records **37 freshness
checks and 21 strict validations**. Project source bytes are unchanged; all
prior task-8 archives and prior active outputs remain hash-verified historical
evidence. No fingerprint redesign, provenance bypass, format compatibility
layer or blind source migration was introduced. Six playable distributions
still contain 19 internal levels, not 19 playable maps. The compiler retains
nonfatal no-valid-irradiance-probe warnings at 1.5 m spacing for Beam room 1 and
the final authoring project rooms 6/10; they are recorded without suppressing
them or altering source placement. Strict/current and native load checks pass.

The final candidate repeats strict Clippy, all 355 Rust tests serially, the
locked release build, nine Python geometry tests, all five asset gates and the
production Luau boundary gate. Native checks load and simulate every rebuilt
non-menu member in a fresh player process with the actual Full lighting variant
and Full reflections requested (**31 members**). This is startup/load/simulation
coverage, not a repeat of full quests or a throughput comparison. The newly
bundled native menu passes all eight parcel-relay checks, retaining the original
item identity across physical pickup and release. The native editor reads the
current bake status and launches the active One Guest package through real Play,
then stops without modifying project sources. The menu run overlapped CPU
baking and establishes function only, not performance.

The editor harness initially failed to parse eight interleaved records because
editor and player shared its optional trace sink. That raw failed attempt is
retained. The final audit instead verifies the exact imported-package hash,
complete world-committed/ready records, separate player diagnostic log and
51.78-second AI simulation state, plus editor Play/Stop and unchanged source.
It does not claim a complete named checkpoint snapshot from the mixed trace;
no production change or repeated native run was needed for this harness issue.

Final candidate 6 SHA-256:
editor `b335143500120207c923d20c7311ccb1dd9f166dfe4c2193e64b2fff13b6fee0`;
player `4966a52bb54f7bc4afa8b5fd43c7cd7dbd81fc54b5aff85e05b9b4a768672f53`;
compiler `9438652eb946467bb508e63a1263adaadb0f5825888c803a5d6052723c1855d3`.
Candidate 6 differs from candidate 5 only in the 20 display-string files;
engine/map-compiler/Cargo/build inputs are identical. Candidate 5's earlier
76-assertion feature audit retains its exact binary provenance and was not
misrepresented as a wholesale rerun on candidate 6. The correction has its own
final-binary receipts. No editor throughput comparison was made.

Raw evidence, helpers, candidates, final symbols, authored acceptance projects,
packages, traces, captures and failed attempts are preserved outside Cargo target
at `C:/Users/Connor Dawkins/Documents/Codex/2026-10-26/task`. Original feature
receipts include `editor-native-audit.json`, `native-settled-*/`,
`native-editor-runtime-inspection-final/` and `native-export-recover-final/`.
Correction receipts are under `editor-acceptance-close/`: `acceptance-audit.json`,
`terminology-audit.json`, `compiler-lineage.json`, `gates.json`,
`assets-final-results.json`, `luau-release.json`, `current-content/summary.json`,
`current-content/packages.json`, per-member current/strict logs,
`native-content/receipt.json` and `native-menu/receipt.json`. Native wording and
editor Play traces are in `native-terminology-close/` and
`native-editor-current-close/`; final source/binary hashes are
`candidate-6-{sources,binaries}.json`. Exact staging commit/remote verification,
final preservation hashes, idle-only Cargo cleanup and custody release are in
the external `ownership-handoff.json`.

Water's accepted optical/performance residual, audio listening/headphone/hotplug/
platform gaps and dense-pause cost, the cold ~2.4-second input stall, CRT review,
broader Python failures/skips and shared-AI's documented small CPU cost remain.
This phase makes no new claim about them. No main merge, release, force push,
version bump, ordinary CI wait, host/GPU settings investigation, iGPU pass or Mac
switch occurred. Editor custody ends here; Backrooms and the remaining queue
return to the parent using GPT-6.1 sol.

### Focused native light-flicker authoring follow-up — 2026-10-26

The selected fixture now exposes **Light flicker** in Simple and Advanced modes
and in Configure. Actual-illumination controls target one fixture or its existing
shared circuit; glow-only controls explicitly target every matching material or
fixture asset. The independent forms retain pending edits across panel/mode
switches and expose bounded timing, burst, start-delay, intensity-depth and preset
choices. Actual illumination uses the existing Power/zero states and Full prepared
light layers; Static holds authored state unless prepared Static switching was
already configured. No renderer, lighting scheduler, package schema or compatibility
layer was added.

Actual presets generate readable editable Luau over the existing `api.light`
contract, with pause/resume/mute/steady/reset exports usable through the native
event-handler picker. Regeneration creates a new file. Edited sources require
explicit ownership transfer; old files/buffers remain intact, and event/manifest
links cannot be orphaned. Stable fixture IDs are pinned. Circuit membership,
deletion, enabled-state changes and overlapping controllers receive validation
instead of silently retargeting a script. Details and limitations are in
`docs/EDITOR.md` under Light flicker.

Settled candidate: `candidate-flicker-1-bin`, preserved outside Cargo target.
Formatting, strict `cargo clippy --workspace --all-targets --all-features -- -D
warnings`, **359 Rust tests** (serial), and locked release workspace/all-features
build passed. Four small source/runtime regressions cover exact scope/history,
custom-source/event ownership, real Luau circuit dispatch, and authored-off/paused
startup/reset. Six Python content-authoring tests passed, as did asset validation,
audio/prop/texture build checks and clip-boundary validation. No production code
changed after these gates; the source hash comparison passed.

Native acceptance used three identical fixtures: one independent and a two-member
circuit. The ordinary editor controls exercised both modes, invalid count refusal,
independent pending glow, undo/redo, a 960×544 window, custom source edits and explicit
regeneration, interrupted fields, save/reopen, native handler linking, Build+Play,
source recovery and reversible Off controls. Recovered level and linked script
bytes match; off/undo/save left the recovered source hashes unchanged. The actual
export passed strict validation and require-current. The native player confirmed
Full lighting, synchronized circuit switching, independent instance timing,
pause/steady/mute/resume/reset, world pause and a normal E interaction invoking the
authored mute handler, without script diagnostics. Fixed-camera captures show the
floor illumination changing: the retained right-floor ROI luminance fell from
90.29 to 37.52 while the independent fixture remained separately controlled.
This is functional illumination evidence, not an FPS comparison. The consolidated
local audit contains **41 passing checks**.

All **16 existing project archives / 32 members**, plus bundled menu and both active
One Guest outputs, passed **35 require-current/integrity checks**. Their preparation
inputs are unchanged: existing `build.rs` excludes editor/UI and test code. Existing
archive bytes remain exact. **Zero shipped-map rebakes** were needed; only the tiny
new acceptance scene was prepared through native Build+Play. No fingerprint rule
was changed or bypassed.

Raw evidence, initial failed harness attempts, native captures, generated/custom
sources, packages, receipts and the gate logs are retained at
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-26/task/editor-flicker/`.
Initial harness window-size/manifest/visible-list selection mistakes occurred
before the completed journey. One initial E ray selected the neighboring Resume
prop; the separate unobstructed E case passed. Those records were not replaced.
Glow-only authoring and source/export preservation were checked, but no separate
quantitative glow-rendering campaign or exhaustive Static/prepared-Static matrix
was added. Prior cold-stall, CRT, listening/hotplug/platform, broader Python,
accepted water/performance and small shared-AI CPU-cost residuals remain unchanged;
this follow-up makes no claim to close them or to establish zero regression.

Delivery records the staging commit/remote equality in the local handoff receipt,
preserves/hashes the candidate, comparison binaries, release symbols and unique
evidence outside target, then performs idle-only `cargo clean`. No main merge,
release/version bump, Backrooms work or integrated final campaign belongs to this
follow-up.

<!-- BACKROOMS_013 -->
## The Backrooms

**Map accepted.** The corrected Full package passed all local and cloud visual acceptance. All 42 physical journeys, geometry/model/clearance gates, native editor workflow, entry/unload/re-entry, 16-archive/32-member current-content checks and 31 inherited native startups passed. Three uncontended Full native performance walks completed on the selected Windows GPU. Required evidence/deliverables were preserved and hash-verified through idle Cargo cleanup.

`projects/backrooms/places.project.json` contains one coherent playable level, `the_backrooms`, with two interwoven floors. The editable current-format scene and explicit bootstrap are retained. Normal editor edits take ownership; the bootstrap must not overwrite them accidentally.

A cloud designer inspected all eight original images and authored the A-H design; the cloud reviewer also compared all eight first native Full views with their originals. Windows implemented that design and did not view original pixels. [The project reference table](../../projects/backrooms/README.md) retains every exact original Library identity and composition. The follow-up cloud reviewer downloaded all eight actual version-1 images, verified the current file IDs/xattrs and compared them against the original pixels; visual acceptance passed.

### Area, height and circulation

The lower footprint is 140 by 140 m, with offset upper galleries at 3.55 m. Gross room floors total 24,928.8 m2; this is not the accessible-area claim. Clear ceilings are 3.4 m, 21.43% above the comparable Office baseline of 2.8 m. Ordinary furniture and doors retain their normal scale except individually audited anomalies.

| Prior playable distribution | Internal levels combined | Sampled standing area (m2) | Conservative support upper bound (m2) |
|---|---:|---:|---:|
| beam | 4 | 1,815.88 | 2,626.00 |
| closing-time | 1 | 278.56 | 396.00 |
| environments | 9 | 11,885.31 | 16,302.90 |
| one-guest-too-many | 1 | 121.00 | 224.00 |
| relay-annex | 3 | 732.31 | 957.60 |
| shape-of-an-exit | 1 | 88.44 | 173.40 |

All six previous playable distributions combine their 19 internal levels. Backrooms measures **20,281.25 m2** in one connected component after conservative hull clearance, versus the prior largest support upper bound of **16,302.9 m2**: at least **24.40% larger**. Nominal standing-centre area is 20,929.0625 m2.

The common audit samples a 0.25 m grid with a 0.8 by 1.8 m standing hull, actual terrain/stairs, wall openings, headroom and transformed solid props. The certified Backrooms calculation expands clearance by another 0.125 m in X/Z to retain each cell extent. Old bounds include every room footprint, overlap and inaccessible space without subtracting obstacles; all old ramps, stairs and floor regions were checked and add no support outside those room unions. No bounding-box padding or disconnected floor is counted. Old sampled estimates do not replay every dynamic door, actor, water state or campaign gate; the conservative upper-bound comparison avoids relying on those estimates as exact accessible areas.

The lower ring A-B-C-D-A and upper ring E-F-G-H-E are joined by separated A-E, B-G, C-H and D-F stairs. The circulation graph has 8 nodes, 12 edges, cycle rank 5 and no bridges, plus local returns in every zone. Ordinary stairs are 2.4 m wide with a 5.6 m run, 3.55 m rise, 20 risers of 0.1775 m and 0.28 m treads. All eight native ascent/descent journeys passed at eye heights 1.6 and 5.15 m.

The 42 real normal-input journeys cover the eight stairs, eight local cycles, eight macro connections, five alternate lower openings, three added architectural loops, G and western upper returns, and eight outer-boundary samples. Starts are test scenario setup; routes use no diagnostic position changes or authored teleports. Four support spans were trimmed to remove coincident geometry, the new chair joints were repaired without changing their outer bounds, and wall corners were corrected before the final fresh replay on a newly compiled Static candidate. Its 42 receipts finished before Full started; the aggregate execution-context receipt corrects inherited concurrent-compiler wording in some harness annotations. Functional timings remain excluded from FPS acceptance.

Current source SHA256: `ff3a188b1d3cf28aeb3158fb26bc503dc4d4f19ef27931ac4c034b41a4ff84f4`. Recomputed area remains unchanged. Historical topology/route receipts remain in `map-evidence/final-candidate-v2/`; final 42-route preflight and clearance receipts are in `map-evidence/final-candidate-v5/`. The successful Full package/captures are in `map-evidence/final-candidate-v6/`; current source/surface diagnostics are in `map-evidence/surface-repair/` and `map-evidence/wall-junction-review/`, outside Cargo target.

### Prop intent and clearance

**25 of 38 prop instances (65.79%)** deliberately use wrong transforms: nine sunken, six tilted and ten wrong scales. The denominator includes all eight model troffers and the ordinary ceiling vent. All 371 generated panel fixtures are catalog `light` instances; columns and walls are architecture. No prop-schema fixture was excluded.

The audit retains every normal and actual full transform, model identity, anomaly class, collision flag, decoded model bounds and collision bounds. All solid props passed clearance against 52,431 actual native player samples; the closest was 0.1699 m at E_chair_1. Non-solid overhead fixtures remain in the denominator. This proves sampled circulation clearance, not every potential avatar position. Deliberate floor intersections remain; accidental z-fighting, traversal obstruction and invalid transforms are not accepted.

| Exact prop ID | X, Z (m) | Normal Y / yaw / scale | Actual Y / roll / scale | Intent |
|---|---|---|---|---|
| `E_chair_0` | 17, 30.5 | 3.55 / 90 / 1 | 3.33 / 0 / 1 | sunken |
| `E_chair_1` | 29, 34.8 | 3.55 / 90 / 1 | 3.63 / 23 / 1 | tilted |
| `E_chair_2` | 42, 30.6 | 3.55 / 90 / 1 | 3.55 / 0 / 1 | ordinary |
| `E_chair_3` | 51, 44.8 | 3.55 / 90 / 1 | 3.33 / 0 / 1 | sunken |
| `E_chair_4` | 17, 45.8 | 3.55 / 90 / 1 | 3.63 / 23 / 1 | tilted |
| `E_chair_5` | 39, 38 | 3.55 / 90 / 1 | 3.55 / 0 / 1 | ordinary |
| `F_solitary_stool` | 9.5, 108.5 | 3.55 / 0 / 1 | 3.55 / 0 / 1.6 | scale |
| `peripheral_chair_0` | 9, 12 | 0 / 0 / 1 | -0.22 / 0 / 1 | sunken |
| `peripheral_chair_1` | 50, 12 | 0 / 0 / 1 | 0.08 / 23 / 1 | tilted |
| `peripheral_chair_2` | 7, 53 | 0 / 0 / 1 | 0 / 0 / 1 | ordinary |
| `peripheral_chair_3` | 51, 54 | 0 / 0 / 1 | -0.22 / 0 / 1 | sunken |
| `peripheral_chair_4` | 66, 5 | 0 / 0 / 1 | 0.08 / 23 / 1 | tilted |
| `peripheral_chair_5` | 133, 53 | 0 / 0 / 1 | 0 / 0 / 1 | ordinary |
| `peripheral_chair_6` | 65, 135 | 0 / 0 / 1 | -0.22 / 0 / 1 | sunken |
| `peripheral_chair_7` | 135, 135 | 0 / 0 / 1 | 0.08 / 23 / 1 | tilted |
| `peripheral_chair_8` | 5, 75 | 0 / 0 / 1 | 0 / 0 / 1 | ordinary |
| `peripheral_chair_9` | 52, 135 | 0 / 0 / 1 | -0.22 / 0 / 1 | sunken |
| `H_cabinet_0` | 84, 91 | 3.55 / 180 / 1 | 3.55 / 0 / 0.55 | scale |
| `H_cabinet_1` | 88, 91 | 3.55 / 180 / 1 | 3.33 / 0 / 1 | sunken |
| `H_cabinet_2` | 91, 91 | 3.55 / 180 / 1 | 3.55 / 0 / 0.55 | scale |
| `H_cabinet_3` | 85, 108 | 3.55 / 180 / 1 | 3.55 / 0 / 1 | ordinary |
| `H_desk_0` | 86, 91 | 3.55 / 180 / 1 | 3.55 / 0 / 1 | ordinary |
| `H_crt_0` | 86, 91 | 4.3 / 180 / 1 | 4.3 / 0 / 1 | ordinary |
| `H_desk_1` | 90, 91 | 3.55 / 180 / 1 | 3.55 / 0 / 0.55 | scale |
| `H_crt_1` | 90, 91 | 4.3 / 180 / 1 | 4.0425 / 23 / 1 | tilted |
| `E_cabinet_0` | 18, 46 | 3.55 / 0 / 1 | 3.33 / 0 / 1 | sunken |
| `E_cabinet_1` | 27, 46 | 3.55 / 0 / 1 | 3.55 / 0 / 1.6 | scale |
| `E_cabinet_2` | 38, 46 | 3.55 / 0 / 1 | 3.33 / 0 / 1 | sunken |
| `E_cabinet_3` | 49, 46 | 3.55 / 0 / 1 | 3.55 / 0 / 1 | ordinary |
| `H_troffer_0` | 84, 102 | 6.947 / 0 / 1 | 6.947 / 0 / 0.55 | scale |
| `H_troffer_1` | 90, 102 | 6.947 / 0 / 1 | 6.947 / 0 / 1.6 | scale |
| `H_troffer_2` | 96, 102 | 6.947 / 0 / 1 | 6.947 / 0 / 0.55 | scale |
| `H_troffer_3` | 102, 102 | 6.947 / 0 / 1 | 6.947 / 0 / 1.6 | scale |
| `H_troffer_4` | 108, 102 | 6.947 / 0 / 1 | 6.947 / 0 / 0.55 | scale |
| `H_troffer_5` | 114, 102 | 6.947 / 0 / 1 | 6.947 / 0 / 1 | ordinary |
| `H_troffer_6` | 120, 102 | 6.947 / 0 / 1 | 6.947 / 0 / 1 | ordinary |
| `H_troffer_7` | 126, 102 | 6.947 / 0 / 1 | 6.947 / 0 / 1 | ordinary |
| `F_ceiling_vent` | 13, 106.5 | 6.947 / 0 / 1 | 6.947 / 0 / 1 | ordinary |

Yaw is unchanged from normal in every row; pitch is zero. Unlisted normal roll is zero. Coordinates are the authored metre transforms; runtime floor resolution is included in the raw world-bounds audit. Troffer origins sit 3 mm below the ceiling independent of deliberate scale, and light anchors clear the measured housing undersides.

### Reference corrections and incident evidence

Corrections brighten actual diffuse materials/light coverage, distinguish the olive/tan/cream/cool carpets, enlarge chevrons to readable normal-distance motifs, add B offset masses and D intermediate reception cores/caps, move E chairs to alternating edges, add F trim/mesh grille and its green seat, make G panels shallow and its aperture elevated, and give H a narrow connected channel and correctly mounted fixtures. New Backrooms furniture winding was corrected locally; accepted shared models were not rebuilt. The sampled Full C study loses the old giant triangular lighting shadow after nine embedded panel anchors were cleared; the final whole-map principal C view is also clear. Three native captures along each of the final 24 m C/H walks were inspected: the former floor defect is absent, and H fixture housings remain mounted across the sampled views, including the intentional scale differences. No exhaustive camera/video sweep is claimed.

The user-reported editor compiler failure was an operator-terminated redundant Export solve (owned child 19092), not an observed Rust compiler/OOM/GPU crash. The first harness also omitted the player benchmark flag and its incomplete trace is retained. Recovery seeded the verified current export, then passed native export and 106.65 seconds of actual player simulation. A later candidate was deliberately stopped after 128.094 seconds (owned child 7992, exit 4294967295) when real traversal exposed a decorative G panel across its stair doorway. Three sections were split around the real opening; corrected B-G walks passed both ways. Original failed receipts and controlled-stop evidence remain intact. A subsequent Full solve was cooperatively cancelled at 2,006.417 seconds (exit 130) after a late surface audit found support overlaps and incomplete candidate coverage. This was a sequencing error; all cheap diagnostics now precede Full. The obsolete Static player was later closed normally after strict dependency validation correctly rejected its old chair bytes; no Full solve had started for that generation. Both incidents retain exact logs.

### Geometry diagnostics before lighting

The former X-only architectural candidate sweep exhausted its 2,000,000-pair bound on separated tiles. A conservative 3D AABB tree now filters distant Y/Z spans while retaining the pair cap, exact clipping and all plane/area tolerances. Input, storage and traversal remain bounded; exhaustion remains an explicit error. A small essential regression covers both the later separated duplicate and adversarial coincident input. The complete native architecture check finishes with zero errors and 294 reviewed, unsuppressed warnings: 201 internal adjacent-room edges, ten supported stair samples, 81 opposite-facing junction contacts, one opposite-facing support sliver and one gap filled by a perpendicular wall.

An independent placed-GLB audit completes 363,278 candidates, with only seven opposite-facing desk/CRT support contacts. Separate new-model audits found and repaired six same-facing chair joints and 18 near-coplanar rail/post contacts. Final chair/stool/vent audits are complete with no same-facing or near-coplanar overlaps; ordinary opposite-facing construction contacts remain. Asset/texture/prop checks and isolated bootstrap reproduction pass. Saved room ownership order is exact; only independent wall insertion order and equivalent number spelling differ in reproduction. These diagnostics complement native walking and final pixels; they do not prove every collision pose or camera condition.

The live corner report exposed centreline butt ends in the upper perimeter. The map-specific bootstrap now assigns corner thickness explicitly: continuous runs own T junctions, and one run owns an ordinary L corner. The saved scene changes 46 wall spans, preserving every opening in world space and all non-wall source. Independent rectangle checks verify 49 perpendicular contacts with zero body overlap; emitted geometry has zero errors and no warning suppressions. Close native G/E/convex/stair views were inspected; E's stepped band is removed and intentional frames/openings remain. Source reproduction is idempotent. The correction uses current map geometry, with no engine or editor-feature change. The prior candidate stopped at its source-hash guard before Full; all 42 journeys then passed on the corrected candidate before Full started.

The next Full process (12656) and all managed controllers disappeared when execution-session supervision was lost. Its last record is 1,825.363 s, direct visibility 15,863,168/27,173,236; no final event or exit code was retained. Peak recorded working set was 3,684,188,160 bytes and commit 3,968,659,456 bytes. No completed Full archive was published. This evidence does not establish a Rust/OOM/GPU crash; the termination cause remains unknown. Source, binaries and accepted 42-route/preflight evidence retained their hashes. Strict/current revalidation of the Static package passed, then only Full was restarted through a supported hidden Windows helper with persistent progress/exit receipts. No quality, budget or resource safeguard was reduced.

### Bounded lighting and current validation

User-authorized Full planning rises from 14 to 40 pages, retaining 1024 px pages, architecture 16 texels/m, the standard transport preset, model sampling and gutters. Low/Medium retain eight-page caps. The actual typed atlas bound rises from 384 MiB + 64 KiB to 640 MiB + 64 KiB; stale Python 320 MiB checks become 640 MiB. KTX payload allowance rises from 512 to 640 MiB to make that bounded headroom usable. Exact shape/layer/overflow/allocation checks, the 64-page decoder, switch-group limits, device limits and 1 GiB ordinary / 2 GiB project aggregates remain. Configured maxima are not measurements.

Formatting, locked release/all-features build, strict Clippy and 360 serial tests passed for the bounded-checker generation with unchanged production-source hashes. The final rebuild against the repaired chair ledger passed: locked release 507.993 s, strict Clippy 19.260 s and 360 serial tests 51.536 s. Preserved compiler SHA256 is `1a6e059a8877e0ca4808274662c65104a1107b4a4708b9bd9cd82161cc830418`; historic hashes remain scoped to their own receipts. An intermediate source-freeze receipt correctly failed after comment-encoding repair; rebuilt frozen gates supersede it. Asset validation passed (644 assets, 392 placeable, 10 themes, zero warnings); texture/prop checks passed with inherited advisories.

The final Full archive is **169,469,609 bytes** (169.469609 MB / 161.618814 MiB), SHA256 `270feeed3e0ff2d0ea9c7545a3cd3c16d880ed488e471ca0a5319b9a81536302`. Actual Full lighting uses **31 pages / 520,093,696 atlas GPU bytes (496 MiB)**, with fallback false. Successful end-to-end Full export took **4,380.394 seconds (1 h 13 min 0.39 s)**; the compiler-reported operation was 4,379.860 seconds. Strict validation took 6.012 seconds and require-current verification 2.004 seconds, both exit zero. Windows OS counters recorded peak compiler working set **9,285,242,880 bytes** and peak commit **9,564,200,960 bytes**; these are compiler process memory, not player resident memory or VRAM. Earlier Full archive and FPS measurements remain comparison evidence only. Affected inherited archives require strict current rebuilds rather than provenance exceptions. No compatibility layer was introduced.

The corrected native A-H Full captures were actually inspected on Windows and replaced the same eight approved Library screenshot artifacts, each backend-confirmed version 1. Cloud original-pixel follow-up passed for all eight. The reviewer accepted slightly washed-out D/E/G tones and stronger A carpet mottling as optional polish, with no further map change or bake required. The unchanged current source passed actual native editor Save/reopen/Build/export/Play/Stop; normal Build cache validation reused the verified Full record, and both exported/root packages passed require-current. No redundant lighting solve or provenance bypass was used. Native editor command-to-idle timings were 157.856 seconds for Build/cache metadata refresh and 4.671 seconds for prepared-record Export, including the recorded 200 ms polling delay. These functional timings overlapped inherited compiler work and are not uncontended throughput measurements. Ordinary Full entry, normal movement, Escape/pause return to the main menu, unload and Explore/map-list re-entry passed; retired Backrooms ownership drained to zero. Restart here means normal unload/re-entry, without an invented restart button or diagnostic player-position change. C/H actual native movement covered 24.2132 and 24.2582 m respectively; all six captured positions were visually inspected, with capture readbacks excluded from FPS acceptance. Functional timings overlapped inherited compilation. All 16 inherited archives / 32 members and three active outputs passed 21 strict validations and 37 require-current checks (all 77 recorded operations exited zero), preserving the 88 inherited project-source files exactly. All 31 non-menu rebuilt members passed actual Full native startups and simulation checks. Each startup consumed only its closed, already verified archive; later independent compiler work could overlap these functional checks, whose timings are excluded from performance comparisons. No full-quest or all-map throughput claim is made. Current packages and native receipts remain outside Cargo target. Staging commit/remote equality is recorded in the local delivery handoff.

### Final Windows performance and preservation

Three genuine normal-input walks ran sequentially after compiler and inherited startup work finished, at actual 1920 x 1080, High / Full lighting, VSync off and Immediate presentation on the NVIDIA GeForce RTX 3070 Laptop GPU. Every ready frame was presented, with zero missed ready frames; sustained rates exceed the 165 Hz display rate. No capture readbacks, graphics bypass or diagnostic position changes are included. No clock/charger/host settings were changed.

| Walk | FPS | Mean loop (ms) | p99 loop (ms) | Mean GPU elapsed (ms) | Peak OS WS / commit (MiB) |
|---|---:|---:|---:|---:|---:|
| A-to-D | 208.05 | 4.806 | 8.356 | 3.998 | 1111.4 / 1867.6 |
| C-pillar-field | 217.97 | 4.588 | 8.670 | 3.928 | 1112.2 / 1867.4 |
| E-parallel-return | 184.50 | 5.420 | 9.919 | 4.698 | 1111.6 / 1867.4 |

OS process counters include startup peaks and are not VRAM measurements. The measured atlas allocation is separately 496 MiB. GPU elapsed is the supported union of pass-stage timestamp spans, including scheduling stalls; it is not active utilization. Earlier matching paths measured 213.39 / 224.75 / 190.13 FPS. Final throughput is lower by 2.50% / 3.02% / 2.96% respectively; the accepted reference corrections remain. These are different map candidates and not an isolated causal claim for a code-level throughput change. The measured difference is retained for later integrated polish; no zero-regression claim is made.

| Fresh process walk | Request to ready (s) | Entry to first scene (s) |
|---|---:|---:|
| A-to-D | 7.718 | 10.859 |
| C-pillar-field | 8.118 | 11.312 |
| E-parallel-return | 7.929 | 11.134 |

These load measurements use fresh processes with previously used filesystem caches, not a cold-storage test. They do not close the accepted cold input-stall residual. Per-run identities, player bounds, memory samples, frame data, load traces and baseline comparisons are retained.

After all task jobs finished, preservation verified 5,767 evidence/deliverable paths (5,721 unique file identities) after Cargo cleanup; required binaries/DLLs/PDBs, native media, current archives and prior comparisons remain outside target. Cargo target is absent. No unique source/evidence or uncommitted data was deleted. The exact cleanup output and before/after hash receipts are retained locally.

Raw evidence/media remain local outside target at `C:/Users/Connor Dawkins/Documents/Codex/2026-10-26/task-2/map-evidence/`. Only the expressly approved eight review images are saved to Library, with same-item replacements for follow-up. Prior water optical cost (p99 4.211 vs 2.988 ms), audio listening/hotplug/platform gaps and dense-pause cost, cold input stall, CRT review, inherited probes, broader Python/platform residuals and small shared-AI CPU cost remain accepted limitations; this map task does not reopen or claim to close them. Integrated polish and final packaging follow separately. The user cancelled iGPU testing.

## Integrated gameplay, visual and performance polish - Windows, 2026-10-27

**Completed bounded integrated acceptance on the accepted Backrooms base
`12b351ea2faef3d4e38a61b3e2999cbbae0a005c`.** The published gap plan combined
Pool movement/carry/water/audio, Beam light/AI response, Home physical handoff,
authored cross-map doors, native editor event/AI/light flows and separate actual
presentation measurements. Accepted Backrooms reference/42-route/startup evidence
was retained; optional visual nits and accepted water work were not reopened.

**Concrete polish.** The game loop published the audio mix twice per frame.
The first native Pool interval records 585 frames / 1,170 mix updates / 335 full
parameter-queue events. `app/mod.rs` now publishes the current listener after
movement and before interaction dispatch; `audio_runtime.rs` no longer repeats
the publication later in the frame. Immediate application-settings updates remain.
Only these two game files changed. No engine/DSP, cue/gain, geometry, asset,
compiler fingerprint, format, compatibility or rendering change was introduced.
The final functional interval records 568 frames / 568 updates / 9 full-queue
events. This removes duplicate work; it does not promise zero overflow during
every fast startup/menu/functional frame or halve the whole audio system's cost.

**Current native combination.** Real E acquisition retains the original duck
through deck-to-basin movement and held underwater crouch; normal mouse throw
releases that same body into buoyancy, and W climbs the shelf to stable dry deck.
Flashlight, headphone-mode mute/unmute, pause/resume and normal menu unload pass.
No body recovery/suspension, mixer error or voice drop occurs in the retained
checkpoints. The initial player/duck placement is explicitly diagnostic setup;
subsequent motion and ownership use native input and the solver. Central 200-ms
post-mixer mute/pause windows are exactly silent; resumed PCM is nonzero and all
captured samples are finite/bounded. The optional tap loses 280,832 startup frames
before its consumer catches up; it is not a complete startup or hardware-latency
witness and establishes no subjective listening acceptance.

Beam's real flashlight holds, releases and reacquires the shared moving actor
without script/mixer errors. No new full four-section campaign or partition-LOS
claim is made from the old timed route. Home's corrected **ordinary-input** route
picks up the original cushion, opens/traverses the real bedroom door, hands it to
the caretaker, witnesses the original physical/render key in the NPC assembly,
retires that assembly only when wholly hidden and saves exactly `["home"]`.
Normal door interactions complete Home -> Hub -> Pool -> Hub. Retired Home/Hub/
Pool prepared/transport owners drain to zero. No diagnostic position, actor,
goal, flag or item mutation enters the Home route or cross-map cycle.

Two unchanged existing editor acceptance sources were copied into isolated
projects. Source checks and read-only geometry/clearance diagnostics preceded
preparation (zero offline geometry errors; broader existing fixture review
findings remain unsuppressed). Native Save/reopen/Build/export preserves all JSON
and Luau bytes. Actual Full Play physically carries/releases the original parcel,
finishes `fetch:complete`, returns to baseline arbitration and actuates its target
light. The editor's last-Play inspector visibly reports that real result. Exact
generated fetch-handler activation is diagnostic because the fixture's controls
overlap. The flicker project uses real E to mute its two-member Full circuit and
passes actual Play/Stop; its independent fixture retains separate timing. Both
level builds and project exports pass strict validation and require-current.
Only these small review projects were prepared; **zero shipped-map rebakes**.
Native captures of Pool, Beam, editor/runtime and interaction evidence were viewed.

An obsolete 3-m/s/instant-stop Home recipe failed against the current Portal-style
controller; an intermediate correction also planned through the chair's actual
contact envelope. Source-aware key timings/clear endpoints fix the harness, and
the final journey completes the favor without changing movement or geometry.
The first AI editor recipe reached Stop before Play preparation had completed;
the failed attempt closed and its owned compiler ended. Completed Build/export,
failure and cancellation evidence remain. A recovered state-aware native journey
waits for compilation and a ready-anchored game Quit, returns normally and reads
last-Play state. Flicker retains one unparsed partial trace line after Stop; its
complete named child witnesses and source/package checks remain available. These
failed/superseded controllers are preserved and excluded from accepted results.

**Uncontended presentation.** Two before and two final runs per scene alternate
on the same exact packages, after all compiler/test/editor work ended. Actual
RTX 3070 Laptop / DX12, 1920x1080 drawable, High/Full, VSync off and Immediate;
normal real delta, no limiter, capture/readback, forced GPU completion, graphics
bypass or performance-time position mutation. Every ready frame is presented.

| Scene | Before FPS (two runs) | Final FPS (two runs) | Final loop p99 ms |
|---|---:|---:|---:|
| Pool fixed Full view | 131.19 / 130.33 | 135.72 / 136.13 | 13.675 / 13.978 |
| Backrooms normal A-to-D walk | 200.70 / 197.03 | 198.62 / 198.60 | 8.207 / 8.169 |

Backrooms sustains rates above the physical 165-Hz display. Pool has material
GPU/presentation cost and does not plateau at refresh. The small two-run cohorts
do not establish a universal FPS gain or a zero-regression claim; Backrooms'
current before/after means differ by about -0.13%. Steady mix publications are
exactly **2/frame before and 1/frame final** in every run. Full-queue events in the
18-second windows are 9/8 (Pool) and 657/543 (Backrooms) before, **zero in all four
final runs**. These steady results do not erase the functional/startup residual.
Raw loop/update/GPU spans, actual presentation identities, player bounds and OS
process memory counters remain in the receipts; OS memory is not VRAM.

**Final gates and currency.** Rust 1.99.1: formatting, locked optimized workspace/
all-features build, strict workspace/all-targets/all-features Clippy and **360 serial
tests** pass. Five asset gates pass: 644 assets / 392 placeable / ten themes,
zero asset-validator warnings and 102 inherited texture advisories. The exact
gated production sources remain unchanged through native/performance acceptance.
A 64-check evidence audit passes; this is not a count of new Rust tests. No
implementation-mirroring test or full-map bake was added to ordinary tests.
All **36 final-compiler require-current checks** pass (32 inherited members,
Backrooms and three active outputs), retaining exact accepted archive bytes.
The Backrooms archive remains SHA256
`270feeed3e0ff2d0ea9c7545a3cd3c16d880ed488e471ca0a5319b9a81536302`.

Final player SHA256: `5f914ad6f3f30cf8c7340a16382e3ac214a30ff6409a9b7c1734f94258d05108`.
Final compiler SHA256: `2d804e9358f5afe64870bd8c2fa1d4fe3b0d0a82875b2b461631e42d414b677c`.
Native media, scripts, raw failures, current-package identities, final binaries/
DLLs/PDBs, comparison binaries and preservation/delivery receipts remain outside
target at `C:/Users/Connor Dawkins/Documents/Codex/2026-10-26/task-3/` and the
unchanged task-2 evidence root. The local ownership handoff records the local staging commit, observed remote
head, delivery status and idle cleanup. Accepted water optical cost, subjective
audio/listening/hotplug/platform gaps, cold input stall, CRT/probe/Python residuals
and small shared-AI CPU cost remain. No host power/security/clock/charger changes,
iGPU work, main merge, release, version bump or 0.13.1 feature work occurred.

<!-- INTEGRATED_POLISH_CUSTODY -->
Idle-only Cargo cleanup removed 6,275 files / 7.5 GiB from the verified ordinary repository target. Post-clean rehash passes all 570 preserved paths / 6,904,792,170 logical bytes. Target is absent; native/build jobs are finished. Local writer custody is released. The external staging push was denied before execution by automatic approval review, which requires trusted direct user authorization for this exact push. The local ownership handoff records both observed commit hashes and the pending publication action; no workaround or retry was attempted.

## Apple Silicon Mac acceptance, 2026-10-27

**Result and scope.** The uploaded staging combination passes the applicable
strict gates and substantive native Mac acceptance after one non-Windows lint
repair and narrow maintenance of existing Python expectations. The exact starting
remote and checkout were `57ce953b1ffad725dcdc1507a0cdb8c598f18263`; the remote
was reverified unchanged at 14:00 UTC. This work used the clean isolated clone
`/Users/connordawkins/Documents/Codex/2026-10-27/task-2/places-mac-validation`.
The older checkout and its unrelated uncommitted data were left untouched.
All evidence paths below are relative to the sibling `evidence/` directory,
outside Cargo's target. This is Mac acceptance, not the complete release bake
inventory or a replay of the accepted Windows 42-route/31-inherited-startup
campaign. No main merge, release, version bump or 0.13.1 implementation occurred.

The actual host is Apple M2 Pro, 12 CPU cores / 19 GPU cores, 16 GB unified
memory, macOS 27.2 (26B123), arm64, SDL 3.4.18, Rust 1.99.1. Native player runs
report **Apple M2 Pro / Metal**. Ordinary finite `caffeinate -i` assertions kept
long jobs awake; no permanent host power, clock or security setting was changed.

**Repairs and gates.** Exact uploaded code first failed strict Mac Clippy because
the `cfg(not(target_os = "windows"))` `heap_diagnostics` stub can be const. Its
only production change is `pub fn` to `pub const fn`, retaining `None` and the
same Windows implementation. No lint suppression or runtime rewrite was added.
`source-custody.json` protects 2,461 production/asset/project/build-input paths;
none changed after the gated compiler and binaries were frozen.

The initial Python suite exposed six obsolete expectations. Four newly exposed
by the accepted Backrooms integration were distinguished from two historical
Lantern/Winter failures by running those exact tests against archived commit
`d5a062866136b9ccc203d2c038d011b6583cca6e`. Repairs retain the two-project curated
lineup while testing the nine-member environment collection separately; distinguish
an elevated Backrooms sightline aperture from walking/door clearance; exercise the
actual 640 MiB plus 64 KiB lightmap record bound, including the 496 MiB witness,
without allocating/decompressing it; protect six initial campfire skeletons, seven
seats and Jun's `quest_seated` quest relationship; and apply lamp-spill assertions
to `square_lamp_0/1`, rather than three deliberately unlit winter-course markers.
No duplicate skeleton was restored and maintained quest content was not regenerated.
The historical bootstrap generator still predates that quest. No new tests or
full-map bakes were added to ordinary tests/CI.

| Final gate | Result and evidence |
|---|---|
| Formatting and strict debug/release Clippy | Workspace/all-targets/all-features, `-D warnings`; pass, `gates-final/`. |
| Production build | Locked release workspace/all-features; pass. |
| Rust tests | **362 passed**, serial workspace; two Unix-specific tests account for the difference from Windows' 360. |
| Full Python suite | **369 tests, zero failures, three optional skips**, 113.24 s; `python-complete/`. The six stale assertions above are closed. |
| Production Luau boundaries | Eleven refusal cases pass using the normal optimized player; `gates-fixed/`. |
| Asset/texture/prop/animation gates | 644 assets / 392 placeable / ten themes, zero asset-validator warnings, 102 inherited texture advisories; `assets/`. |
| Diff and source identity | Clean diff formatting, identical production source and preserved executable hashes after native work. |

**Freshness and preparation.** Runtime package portability and preparer currency
are different contracts. `src/engine/package/world.rs` consumes the package's
catalog closure and validates format/capabilities/dependencies/hash/bounds; it does
not require the authoring compiler's source fingerprint to match the current
host. `build.rs` fingerprints target/features and production/compiler sources;
`src/map_compiler/mod.rs` rejects changed compiler provenance under `--require-current`.
An intact Windows package could therefore be portable at runtime while honestly
stale against the Mac preparer. No Windows archive was available for transfer.
Normal Mac preparation, with full provenance and original quality, was necessary
for currentness acceptance. No source-freshness override, quality reduction or
provenance rewriting occurred.

The two complete-project exports started once, continued during the UI approval
wait and finished normally. Backrooms: 8 workers, **58m22s**, exit 0 at 09:24:08
UTC. Environments: 4 workers, **1h54m38s**, exit 0 at 10:26:28 UTC. Both preserve
normal Static/Full variants. Logs and receipts remain under `compiler/*-build/`.
No visible compiler job was cancelled or duplicated. Integrity validation and
all **ten member `--require-current` checks** pass with the final Mac compiler
(`compiler/final-package-validation/`, `compiler/all-member-currency/`).

| Preserved Mac package | Bytes | SHA256 |
|---|---:|---|
| Backrooms | 169,478,050 | `ddbd92a89ae9c6d1f9aec2ecac3ce97941dac93f1230f9cbbf8e903a1aecc56f` |
| Environments | 435,943,849 | `a11a8add18129e66d573612376ac557ffdb6b72cd69804bef75ff46087d9f091` |

These newly prepared archives have their own Mac provenance and are not claimed
to be byte-identical to accepted Windows archives. Smaller native-only AI,
navigation, celestial and flicker fixtures also used ordinary compiler paths.
They are validation artifacts and must not enter the release map directory.

**Native functional acceptance.** Captures and state traces are functional
evidence, excluded from FPS cohorts. `native/final-functional-audit.json`
independently checks the raw normal exits and state histories where a generic
single-member/fixed-frame benchmark helper intentionally cannot report success.

| Area | Observed Mac result |
|---|---|
| Current maps / large atlas | All **ten** current Full member startups pass: Backrooms plus Hub, Home, Pool, Office, Arcade, Lantern Hollow, Winter, Beach and Aero. All ten captures were visually inspected. Actual Metal draws and resident Full atlases, finite movement, no reset/script/fatal error and normal exits; `native/current-startups/`. Backrooms loads **31 pages / 520,093,696 bytes (496 MiB)** without fallback. |
| Backrooms movement/collision | Normal W input climbs the 20-step stair from eye Y 1.6 to 5.15 m, then returns to the lower deck, grounded and reset count zero. Normal C-area and upper H-corridor walks cover **21.74 m / 21.87 m** at the correct floor heights. No post-spawn player mutation; before/mid/after captures and traces retained. The Windows exhaustive route evidence remains applicable; this is representative native platform coverage. |
| Pool interaction / water / physics | With one initial diagnostic player/duck setup, ordinary E/W/crouch/throw input keeps original body `9223372036854775845` held into underwater crouch, releases that same body and lets it float at Y -0.161 m (submerged fraction 0.094, finite settled velocity). Ordinary movement reaches dry deck grounded at eye Y 1.6, zero resets. Visible floating-duck/water captures reviewed. The held-underwater capture is not evidence that the duck is visible in that particular camera view. |
| Lifecycle / settings | Native Pool mute/headphones, unmute, pause/resume and normal return to bundled menu pass; the original Pool body disappears. No claim that total residency returns to a universal baseline. Three shared-AI reloads separately clear live ownership/tasks/reservations with distinct epochs. |
| Shared AI | Existing acceptance plan/assertions retained, only local root/settings select Full lighting/reflections: **98 assertions across five native cases** pass, including fetch/ownership, light cargo, interruptions/ragdoll, senses and reload. `native/shared-ai-full/` and harness-custody JSON retain exact commands and hashes. |
| CoreAudio | Real MacBook Pro Speakers / CoreAudio, 48 kHz stereo, **32 lifecycle cycles**, finite PCM, mute/latency checks; zero callback allocations/deallocations and zero voices/clips/errors after unload. `native/audio-audit/`. Subjective listening and physical hotplug remain untested. |
| Lights / flicker authoring | Actual native editor selected light 0, applied/saved the existing flicker preset and completed normal Full Build. Runtime Full snapshots contain **61 checkpoints: 57 on / four off**, with the other five lights unchanged and no script errors. Saved visible captures all sampled ON; their filenames do not establish a paired OFF visual comparison. The real Full runtime transitions and the inherited Windows visual evidence are preserved separately. |
| Native editor | Open, Simple/Advanced, transform, undo/redo, Save, Build, complete-project export and reopen pass on an isolated celestial-project copy. All four authoring files recover byte-for-byte. Real Play child reaches Full Metal world, exits normally and returns to editing; a later active Play/Stop round trip also passes. Initial premature import-only Play attempt is retained and excluded. `native/editor-roundtrip-audit.json`, `editor-play-complete/`, `editor-stop/`. |
| Native compiler / portable payload | Normal `tools/package.sh --with-editor` payload includes player/editor/compiler and bundled SDL/assets. An own copied review app launched the exact compiler GUI; after owner Allow, the actual Bake button produced **370,503-byte** output and visible Bake complete. The packaged compiler validates and verifies it current from `/tmp` with `PLACES_ASSET_ROOT` unset. Official map-free payload, separate review bundle, exact output and logs are preserved under `portable/`. This is not an all-map release bundle. |

The original compiler-app Computer Use request waited from 08:47 until owner
Allow around 13:28 UTC. Background bakes continued and were reconciled before
resuming; no restricted app interaction was routed through another interface.
The GUI logged a sandbox-extension warning but successfully completed its bake
and independent currency validation. The initial standalone Pool package was
correctly refused because its transitions require the whole project; that owned
failed player diagnostic was explicitly terminated, logged and excluded. The
successful run uses the complete Environments project. Pool's deliberate menu
transition and the stairs' early quit produced benchmark-helper exit 1 despite
native exit 0 / `shutdown_complete`; their raw functional histories were audited,
not presented as green FPS receipts. Later timed C/H/flicker helpers all exit 0.

**Capture-free VSync-off performance.** Six serial runs began after all bake,
test, editor and acceptance work ended. Each uses High textures/filtering,
Full lighting/reflections, bloom, requested VSync off and actual applied
**Immediate** mode, **1920x1080 physical drawable** (960x540 logical), real delta,
120 warmup frames and 18 measured ready seconds. Windows remain natively focused
and unoccluded. There is no capture/readback, forced completion, graphics bypass,
frame limiter or measurement-time position mutation. Backrooms uses ordinary W
movement, Z 68 to 104.23 m. `performance/accepted-summary.json` retains per-run
mode/settings/source identities and raw loop/update/GPU/RSS metrics.

| Scene | FPS, two runs | Loop p99 ms, two runs | Mean CPU update ms | Mean GPU elapsed ms |
|---|---:|---:|---:|---:|
| Pool fixed Full view | 120.00 / 119.99 | 9.264 / 9.173 | 0.134 / 0.135 | 3.541 / 3.600 |
| Backrooms normal walk | 120.00 / 120.03 | 9.131 / 9.289 | 0.042 / 0.050 | 1.253 / 1.460 |
| Arcade dense view | 119.87 / 119.89 | 9.452 / 9.439 | 0.778 / 0.766 | 6.239 / 6.308 |

All 2,154–2,164 ready frames per run present; missed count is zero. However, all
cohorts plateau at the physical **120 Hz** refresh rate. A separate explicitly
rendering-bypassed surface control also runs at **120.002 FPS**, with mean render
0.002 ms and swap/present 7.470 ms. This supports a presentation-pacing ceiling;
**uncapped application throughput and throughput gains remain inconclusive**.
The control is diagnostic, not a playable performance result. GPU elapsed spans
are not active utilization. Native RSS is OS process memory, not VRAM: Pool
534–536 MB, Backrooms 1,224–1,227 MB and Arcade 1,052–1,054 MB. No cross-platform
FPS ratio, universal zero-regression claim or new optimization is inferred.

**Final platform/release handoff.** The const repair is excluded from the Windows
implementation, and the other changes only repair existing Python expectations.
A Windows gameplay rewrite is not required. Strict final Windows build/tests and
current package checks remain necessary: the compiler's global source fingerprint
changes even for a target-gated source edit, so old package currentness must not be
asserted for the final compiler. Parent coordinates final CI and the ordinary fresh
Windows release preparation; no extra release bake was started during this Mac phase.

The eventual upload inventory is **seven playable projects**: `backrooms`,
`environments` (including Home / Quiet Rooms), `beam` (It Only Exists in the Beam,
four chapters), `closing-time`, `one-guest-too-many`, `shape-of-an-exit`, and
**After Hours - Rooms Beyond** at `projects/untitled-1792640069720`
(`room_1792640069720`). **Relay Annex is excluded from release uploads; its
repository source remains.** `levels/lineup.txt` is the curated default, not the
complete release inventory. `package.sh` includes every `.placesmap` from the
supplied map directory, so development labs, gyms and review fixtures must stay
outside that directory. Backrooms/Environments here are Mac acceptance preparation,
not proof that all seven release projects were freshly baked or played on Mac.

Linux, subjective listening/hotplug, uncapped Mac throughput and the previously
accepted Windows residuals (water optical cost, cold input stall, CRT/probe limits,
startup/functional audio publication-queue pressure and small shared-AI CPU cost)
remain honestly scoped. Native Pool audio queue-full observations are retained;
no zero-queue-pressure claim is made for every functional/startup interval.
The previously documented six Python expectation failures are closed by this stage.
No pending native permission or demonstrated unresolved Mac blocker remains.

Player SHA256: `1625930b87b4f76b9ed651708b63c3306ca1baa7a19c9530485fb3e895e81e80`.
Compiler SHA256: `e2cfaca6b1a1696f223f52ca33ce671a6a0151480241ec5507c25c3ebb7139a0`.
Editor SHA256: `72515e63d0a845442f9c750eee00c75630d9ac9f7822a47374aa134fa54e1ba1`.
All ten direct release executables, prepared packages, official portable payload,
review-only artifacts, raw failed attempts, logs, traces and native captures remain
outside target. `FINAL-HANDOFF.json` records exact local/remote delivery and
post-clean custody without creating a second canonical prose report.

<!-- MAC_ACCEPTANCE_CUSTODY -->
Idle-only `cargo clean` completed at 14:04 UTC, removing **8,141 files / 7.7 GiB**
from this clean validation clone. Target is absent. Post-clean SHA256 verification
passes **8,218 preserved files / 2,800,699,686 logical bytes and 24 symlinks**, with
zero differences. A fresh two-second Full Backrooms native smoke from preserved
executables/packages exits normally after cleanup. Native/build jobs have finished;
the finite keep-awake assertions have no permanent host effect. Staging commit,
observed remote delivery and final writer custody are recorded in
`evidence/FINAL-HANDOFF.json` outside target.


## Final Windows release-candidate preparation (2026-10-27)

This phase reconciled the Razer checkout from Windows polish commit
`57ce953b1ffad725dcdc1507a0cdb8c598f18263` and safely fast-forwarded the authorized
`0.13.0-staging` branch to the accepted Mac commit
`9fca431855e002774516559508af2bc8735eadef`. The observed remote matched that exact
commit. The final delivery commit, exact remote equality, closing hosted CI result
and writer custody are recorded in `FINAL-HANDOFF.json` beside the evidence below.
This phase performs no main merge, tag, public release or 0.13.1 bump.

Evidence is preserved outside Cargo target at
`C:/Users/Connor Dawkins/Documents/Codex/2026-10-27/task/rc-evidence`. Raw failed attempts remain alongside successful receipts.
`CHECKPOINT.json`, job receipts and five-second heartbeats retain stage, command,
PID, timing and log custody. The single release-project whitelist is
`tools/release-projects.json`: seven projects, 18 members. Relay Annex source is
preserved and excluded from downloads, as are development labs, gyms and review
fixtures. The smaller `levels/lineup.txt` is not the release inventory.

**Strict gates and narrow repairs.** All commands use the locked repository
toolchain; both Clippy profiles deny warnings. Rust tests run serially.

| Gate | Result | Controller seconds |
|---|---|---:|
| `format` | passed | 5.01 |
| `release` | passed | 595.34 |
| `check` | passed | 130.08 |
| `clippy` | passed | 45.04 |
| `clippy-release` | passed | 65.04 |
| `rust-tests` | passed | 290.25 |

The Windows Rust suite passes **360 tests**. The complete Python suite passes
**369 tests**, zero failures/errors, **18 optional/platform skips**, in 161.845 s.
Asset, audio and maintained generator gates pass. Native cheap `check-project`
preflight for all seven projects and all 11 production Luau refusal cases finish
before the expensive exports. A supplementary catalog closure audit passes
**7,588 references across 18 members**.

Windows-only failures were repaired at their cause: canonical POSIX ledger keys,
CRLF-independent historical text hashes/fixtures, native path comparisons,
directory-symlink privilege handling, shell rollback injection, native command
launch fixtures and real Windows cancellation signaling. Assertions and original
hash pins remain. The cancellation test's two confirmed orphan workers were
reconciled before the successful suite/preflight/bakes. No host security, GPU or
power setting was changed. A new harness's environment-case lookup was corrected
before packaged native tests; its failed source-audit attempt remains recorded.
The player's supported `--paths` / `--list-levels` checks pass. A mistaken
`--version` harness check launched normal gameplay and reached its deadline;
that owned process was stopped and its receipt retained. Remaining checks resume
from that point without a product change, repack or release-map recomputation.
The runtime intentionally records successful `api.notice` output in its bounded
diagnostics too. The Closing Time title initially tripped the harness's empty-list
assumption. Exact source-proven notices are now classified explicitly; positive
and negative checks retain rejection of VM errors, budget failures and wrong-level
messages. The original normal Full run is audited without replay or package edit. Shared
neutral fallback materials are reported separately from missing textures; all
18 members have zero missing-texture counters and no Full-lighting fallback.

The earlier hosted failure was a test-checkout boundary: maintained quest tests
compile authored Luau with `include_str!`. Test-aware check/Clippy/test gates now
use the complete source checkout. Formatting and production compilation still
prove the engine-only checkout. No test was removed and hosted CI performs no
full release-map bake. The closing hosted run targets the final delivered commit;
its exact result belongs to the final handoff receipt, separate from native QA.

**Fresh preparation and provenance.** The old accepted Windows package's integrity
still passes, but the current compiler reports `compiler preparation code changed`
and requires preparation after the Mac const-fn correction. Normal Static/off and
Full exports were therefore freshly computed with the preserved strict-build
compiler, eight workers, one project at a time. Logs retain real receiver/direct/
bounce/filtering/reflection stages. No prior Windows or Mac lighting was copied
into the release directory. Completed current outputs were retained; documentation,
notice and packaging-tool changes did not trigger another lighting computation.

| Project / archive | Members | Export duration | File bytes | SHA256 |
|---|---:|---:|---:|---|
| The Backrooms / `backrooms.placesmap` | 1 | 79m42s | 169,469,607 | `35424865bca1a3c5a8f9abe0d7d948ab9569f691e5390e7f8c279f7edaea290d` |
| The Quiet Collection / `environments.placesmap` | 9 | 99m23s | 435,973,168 | `7705f37242b10e9f0f4ad9ed6bde0f1c938510ec4efd8705984f20a9392cf360` |
| Beam (four chapters) / `beam.placesmap` | 4 | 13m55s | 56,481,178 | `dcf8f4b151c2d870cf5e239de5a7834592bec92460bfa3441f8687ddecb2ee08` |
| Closing Time / `closing-time.placesmap` | 1 | 7m55s | 32,807,634 | `4698662132179dc75dae2dbaae037985adf24e87dd42dc3dd0068a6e2c933fb1` |
| One Guest Too Many / `one-guest-too-many.placesmap` | 1 | 3m10s | 14,825,181 | `c79f5c828c1ace1a29375c6ecb7729b177dfb4794673e2925c4a1a4aa404b7c0` |
| The Shape of an Exit / `shape-of-an-exit.placesmap` | 1 | 3m20s | 10,691,646 | `824be2547326c38389f3ecbea052ebf31d62dc14b0c867ce701b9ee1181b60c5` |
| After Hours - Rooms Beyond / `after-hours-rooms-beyond.placesmap` | 1 | 18m01s | 44,413,559 | `01124c77b8c5fc3d0917c47c37ff5ccf2938e2f42b14bb91fbb001429f986bcd` |

All seven archives pass native integrity/dependency validation and every member's
`verify --require-current`. All 36 variants are present with zero Full fallback.
The exact map-file total is **764,661,973 bytes = 764.662 MB = 729.238 MiB**;
this excludes the outer ZIP, binaries and shared assets. After Hours' project,
level and Luau source match the last accepted Windows archive byte-for-byte and
retain the documented polished content. All seven later recoveries also match
their current authoring sources exactly. The application-owned menu is separately
verified/prepared and retained with its old resource backup and new provenance.
At the menu publication step, the source proof changes only its generated
`assets/shared/menu/scene.bin`; later provenance documentation is identified
separately. Authored map, Rust, image and model sources remain unchanged.

**Windows portable distribution.** `tools/package-windows.py --with-editor`
includes the actual x64 player, native editor and compiler, six required runtime
DLLs, full runtime resources and the seven project archives. It validates the
selected packages against staged resources and all 18 discovered member IDs.
Protected roots, existing products and actual Windows junction redirection are
refused before staging; isolated sentinel data remains intact. The exporter
preserves SDL and Windows SDK/Visual C++ notices, exact runtime identities and
`PAYLOAD_SHA256.json`. DLL origins/hashes and native PE imports are retained.

The ZIP is **978,508,389 bytes**, SHA256
`0a25b0690c2f2e691d0357377f1883b9bd51be28298ef60d1930d74c3b0fe608`. CRC validation, clean extraction and every staged/extracted
payload hash pass. Native workflows leave the extracted installation unchanged.
`upload-inventory.json` names exactly ten prepared Windows upload files: this ZIP,
seven standalone maps, release notes and `SHA256SUMS.txt`. Nothing was published.

| Executable | SHA256 |
|---|---|
| `places.exe` | `44f1d8db149f92d7dd2b28425ed30197c495edc12505d849f07e6f87ba5e7201` |
| `places-editor.exe` | `5eecd86d3a6793bebc97d857f61af18596d5f71b6633f6a92ea4a4b618e95a2e` |
| `places-compile.exe` | `ad677b062722a80d9f26cd64627cff014d53a355beb2c03604a99fba3d329b70` |

The optimized build's recorded source HEAD is `9fca431855e002774516559508af2bc8735eadef`;
the final source ledger confirms identical gated Rust/Cargo inputs. Packaging
records its actual worktree status, prepared assets and separate build receipt,
rather than pretending that later delivery documentation existed at compile time.

**Clean extracted-folder native acceptance.** All 18 members start and render Full,
accept ordinary movement and exit normally. Captures are visually reviewed and
excluded from timing. Functional game/editor/compiler EXEs use executable-relative
assets from the extraction, outside-repository working directories, isolated user
state and a Windows-system-only PATH. Development SDK/tool PATH entries cannot
mask a missing packaged DLL. These are Razer native checks, not clean-OS installer
or downloaded SmartScreen acceptance.

Backrooms uses **31 resident Full pages /
520,093,696 bytes**, without fallback.
The fresh Pool carry/crouch/underwater/throw/buoyancy/dry-exit/pause/unload workflow,
Beam flashlight/AI response, ordinary Home carry/handoff/whole-assembly retirement/
save, and Home-Hub-Pool-Hub door cycle pass **12 semantic checks**.
Previously accepted action data is explicitly identified; executables, assets and
all prepared maps are the fresh extracted candidate. Pool/Beam diagnostic setup
is retained honestly; normal input drives the later interactions. No exhaustive
new quest campaign, subjective listening, PCM or hotplug claim is inferred.

The extracted compiler recovers all seven complete projects, checks their sources,
validates every archive and verifies all 18 members current without repository
runtime resources. Native editor production control dispatch passes mode changes,
Save/reopen, Full Build, complete-project export, actual Full Play child and normal
Quit/return. Source bytes remain exact. The Play child history is an intact
1,278-event byte prefix through normal shutdown. Its
optional shared trace has padding only after that point; raw bytes remain and
the parent returned-to-editing state independently corroborates the lifecycle.
No native operation is replayed to repair that diagnostic artifact. The actual
packaged compiler GUI Bake action, activated through approved keyboard navigation,
prepares a separate small review map:
**1,952,482 bytes**, SHA256
`b10277d770b9dd75d211b0ca8cec2785b32eb2d322bd33a04dd99b4e646a8a0c`. Its native integrity/currentness checks and normal close
pass. Desktop screenshot capture timed out despite approved app access; compiler
GUI appearance is not claimed. Its real Bake-complete log and prepared output
establish the functional workflow. Review fixtures stay outside the release directory.

**Measured Windows presentation.** Six serial native runs use a quiet measurement cohort after release building and
baking, with no competing GUI workflow. High textures/filtering, Full lighting/reflections, bloom, real delta,
120 warmup frames, a 22-second ready window, requested VSync off and actual
Immediate presentation use **1920x1080 physical drawable** on the **NVIDIA GeForce
RTX 3070 Laptop GPU**. There is no capture/readback, limiter, forced GPU completion,
graphics bypass or competing owned build/bake. The unchanged copied helper's
initial focus/Return actions label receipts diagnostic; they do not disable
rendering. Backrooms movement uses ordinary held input. Timing explicitly names
the extracted resource root and preserved bounded diagnostic manifest reader;
functional discovery checks above need no asset override.

| Cohort | FPS | Loop p99 ms | Mean update ms | Mean GPU elapsed ms | Ready / missed |
|---|---:|---:|---:|---:|---:|
| Backrooms-walk-1 | 180.67 | 10.626 | 0.156 | 3.307 | 3981 / 0 |
| Backrooms-walk-2 | 190.47 | 10.512 | 0.143 | 3.622 | 4190 / 0 |
| Pool-full-1 | 128.97 | 16.029 | 0.534 | 6.737 | 2827 / 0 |
| Pool-full-2 | 132.79 | 16.013 | 0.515 | 6.353 | 2914 / 0 |
| Arcade-full-1 | 95.92 | 14.328 | 1.358 | 9.895 | 2092 / 0 |
| Arcade-full-2 | 94.89 | 14.533 | 1.358 | 10.034 | 2066 / 0 |

The six current-candidate Razer runs measure Backrooms walking at 180.67/190.47 FPS, Pool at 128.97/132.79 FPS and Arcade at 95.92/94.89 FPS. All use actual 1920x1080 Immediate presentation and have zero missed ready frames. Scene-dependent results above and below 120 FPS are not a single refresh-rate plateau. They establish this candidate's presented performance on this host, without a matched historical performance-gain claim or a cross-platform ratio. GPU timing spans include scheduling stalls and are not active utilization.

GPU elapsed spans are not active utilization; OS process RSS is not VRAM. No
cross-platform FPS ratio or universal throughput gain is claimed. Mac Immediate
1080p results remain refresh-paced and inconclusive for uncapped throughput.
Linux/clean-OS installers, subjective listening/hotplug and the accepted Windows
water optical cost, cold input stall, CRT/probe limits, startup/functional audio
publication-queue pressure and small shared-AI CPU cost remain scoped in this
report. No cancelled iGPU campaign was resumed.

**Artifact custody and cleanup.** All local phase build/bake/native jobs finished
before cleanup. Preserved binaries/PDBs, maps, packages, logs, traces and captures
remain outside target. The guarded Cargo cleanup records:
`Removed 7553 files, 8.1GiB total`. Target is absent, and post-clean SHA256
verification passes **4,200 preserved files /
7,282,808,958 logical bytes**, with zero differences. Final
delivery/hosted-CI/writer-custody status is retained in `FINAL-HANDOFF.json`.


## Prerelease publication and development handoff (2026-10-28)

[Places 0.13.0 Windows prerelease](https://github.com/csd113/Places/releases/tag/v0.13.0) is published with exactly three
verified assets. Annotated tag `v0.13.0` and remote main resolve to the validated
commit `b24361755e15c1a96d5b56d115499b9d4a9ef07f`. The closing
[CI run](https://github.com/csd113/Places/actions/runs/40600373906) passes all
18 steps, 362 Rust tests and 369 Python tests (16 optional/platform skips).

The Windows ZIP is 978,508,389 bytes, SHA256
`0a25b0690c2f2e691d0357377f1883b9bd51be28298ef60d1930d74c3b0fe608`.
All three uploaded asset names, sizes and GitHub SHA256 digests match the final
upload inventory. The seven embedded maps total 764,661,973 bytes. The download also
provides release notes and SHA256SUMS.txt; no private evidence or Annex package
was uploaded. The corrected three-asset release supersedes the earlier ten-file
prepared upload inventory; standalone map outputs are retained locally. Release-note links are pinned to the validated source commit.

After publication, `0.13.1-dev` was created/pushed from updated main and verified
before the fully merged old local/remote `0.13.0-staging` branch was deleted.
The version-only development change updates Cargo/lock/release-inventory version
to 0.13.1 and uses focused metadata, formatting and consistency checks. Existing
game/editor/compiler displays derive from CARGO_PKG_VERSION. Published 0.13.0
binaries and prepared maps remain unchanged; this handoff implements no editor
queue features. Detailed publication, server digest and branch receipts remain
under `C:/Users/Connor Dawkins/Documents/Codex/2026-10-27/task/rc-evidence/publication`.
