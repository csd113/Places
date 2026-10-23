# Places 0.13.0 implementation and validation report

This is the consolidated report for the approved 0.13.0 queue on
`0.13.0-staging`. Sections retain the evidence and limitations recorded at each
stage; earlier next-stage statements are historical handoffs. Raw local receipts
and direct binaries remain outside `target` in their original stage directories.
No main merge or release is implied by this staging report.

The [final verification and cleanup entry](#final-verification-and-cleanup--2026-10-22)
records the completed macOS checks, final local packages, media and cleanup.
Presented macOS FPS gains remain inconclusive. The Windows entry below records
native Razer acceptance and its remaining limits; Linux remains unverified.


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
