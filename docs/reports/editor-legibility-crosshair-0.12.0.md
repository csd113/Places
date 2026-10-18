# Editor legibility and five-dot crosshair — 0.12.0

This queue stage stays on `0.12.0-staging`, starting at `965a4ac9`. No main
merge, release, later queue stage or game-mode source change is included.

The native before captures confirmed two concrete causes: editor glyphs came
from the game's 8×8 nearest-sampled HUD, compressed to 3.5×4.5 reference pixels,
and its SDL window did not request the native Retina drawable. The editor now
uses a separate committed 512×448 antialiased Roboto Mono medium atlas with
clamped linear coverage, 4×7 reference glyphs and native high-density output.
Source font/OFL license and an explicit offline rasterizer are retained under
`tools/fonts/`; runtime only loads the PNG. The game's bitmap font/hand sheet,
world texture settings and shaders remain unchanged.

The slate/mint identity, left scene/transform column, central viewport, grouped
project sheet and build/view toolbar remain. Labels are vertically centered,
muted text has more contrast, toolbar widths fit the larger font, long values
use ellipses, and multiline source/diagnostic spacing and caret positions match
the new glyphs. Tab/Shift-Tab traverses visible enabled controls; Enter activates
the mint-outlined focus. Numeric/script text input keeps its editing keys.

The game draws five small white dots at 40% alpha with a faint 28%-alpha dark rim: centre plus four cardinal
dots, white radius .45, outer radius .62 and spacing 2.5 in the 480×272 reference space. The centre
accounts for the actual drawable and odd letterbox remainders. This is simple
UI geometry sampling the existing white cell, with no new runtime texture
construction or target/physics change.

Top-level Boolean `crosshair` defaults to true on existing maps. Project →
Map crosshair toggles the selected map through ordinary undo/save history.
`/crosshair on|off` overrides that map default for the current visit; help is
registered in `/help` and `/crosshair help`. Real map loads/reloads clear the
override. Respawn/world reset and graphics-only rebuilds retain it. Menus,
loading, console, guidance/death/win panels and third person hide the reticle.
Holding retains it; the interaction hand remains independently tied to the
precise eligible nearby aim target and is hidden while carrying. No new
in-game tutorial prose was added.

## Acceptance and custody

Evidence root: `local/editor-crosshair-0.12.0/`. Direct native binaries only;
repository assets and host SDL3 are required. Native execution is macOS/Metal,
not Windows/Linux certification. Native SDL window raise and focus receipts
verify the actual editor/player window; the previously denied Terminal UI
binding was not retried or bypassed.

- Matched native before/after ordinary window sizes: 960×544, 1200×800,
  1440×816. Retina after drawables are exactly twice the logical dimensions.
  Project, inspector, geometry sheet, scripts and visible keyboard focus are
  captured; the viewport retains its existing region.
- Native per-map toggle/save/reopen/selection and export/import recover the
  manifest and both JSON files byte-for-byte, including opposite Boolean
  defaults. Undo/redo is covered by a tiny existing-document fixture regression.
- Native crosshair receipt checks default maps, override on/off, invalid usage,
  console/help, low texture/graphics change, nearby target/pickup, third person,
  pause/resume, respawn, transition and same-map reload, with actual focus.
- Small CPU regressions cover strict command syntax, map override precedence and
  clearing, exact odd-drawable centring, and independent map history/reopening.
  They use no GPU, asset load or map bake. Ordinary CI still bakes no maps.
- Mannequin R2 bytes, Spooner-Man bytes, Beam black-preset metadata and both mode
  source projects are verified unchanged against the starting head.

Failed setup attempts remain labeled separately: sandbox-native launch had no
available display; an early editor replay redundantly opened an already-open
Project sheet. The corrected supported SDL input route completes the checks.

The editor atlas is embedded from its committed PNG and deliberately stays
outside the world catalog. A compatibility check caught and removed an unnecessary
catalog entry before shipping: the installed catalog remains byte-identical, so
existing game packages retain their dependency identity. Its 8:7/16×8-cell contract
is explicit in the asset specification; world texture budgets are unchanged. No FPS comparison is made: captures, VSync and concurrent
build work are not uncapped throughput evidence. No external non-Retina display
or Windows/Linux native run was available in this stage.

The broad editor/UI issues remain open: their additional platform and workflow
acceptance is beyond this completed legibility/crosshair stage.

Local final gates pass: strict all-target/all-feature Clippy, locked all-feature
release build, formatting, 143 Rust tests (0.28 seconds execution), and 369 Python
checks (98 seconds, three supported skips). Asset/catalog, prop and texture
checks pass with the previous 96 texture advisories. Native editor acceptance
has 19 assertions and initial crosshair acceptance has 27 assertions/19 captures.
The original world catalog remains exact SHA-256 `301c0a0c…`.

Library screenshots: [before, 960×544 logical](https://chatgpt.com/api/library/files/libfile_f20c22026de08191afc8807d8c9a481c/download),
[after, same window with Retina drawable](https://chatgpt.com/api/library/files/libfile_b943bb0a02e4819186120d9106b6872a/download),
[before, 1440×816](https://chatgpt.com/api/library/files/libfile_67ef62cd595481918d63428f6ddc1f1a/download),
[after, 1440×816](https://chatgpt.com/api/library/files/libfile_d4325a475030819198f0a643d2e1b8dd/download),
[keyboard focus](https://chatgpt.com/api/library/files/libfile_41b7d21049688191aecf1a4e44859d90/download),
[five-dot crosshair](https://chatgpt.com/api/library/files/libfile_0566c028ff60819180162b2970a86d0d/download),
[target hand plus crosshair](https://chatgpt.com/api/library/files/libfile_11a26f56847c8191b7031699a2b1a67d/download).

Both original integrated game packages import unchanged with the original
catalog hash. Final-player Beam death/double immediate reset passes all 44
existing assertions; Closing Time's ordinary-input win route passes all ten.
Native Beam death/guidance screens hide the crosshair and the resumed world
restores it. Closing Time's completion remains its existing live world label,
so gameplay retains the reticle. Full asset audit passes: 381 models, 417
embedded PNGs, 581 standalone PNGs, zero errors.

## Authorized refinements and menu images

Connor requested a subtler reticle and reliable contrast in darkness and on
light surfaces. White opacity is reduced from .50 to .40. Each dot has a narrow
.62-radius dark rim at .28 alpha around its .45-radius white centre. This is a
fixed contrast-preserving treatment, with no animation, brightness pumping or
frame-to-frame noise. The UI pass currently receives no background texture or
luminance history; adding scene copies/history bindings for dynamic adaptation
would widen this small HUD change. No costly GPU readback or synchronization
is introduced, and no claim of actual luminance adaptation is made.

The additional authorized level-icon work reuses stage 7's exact per-level and
project thumbnail references, content-addressed PNG blobs and source recovery.
The editor offers a native PNG chooser, visible path entry, full 16:9 preview
and removal. Removing an entry image also clears its package fallback; shared
PNG files are retained so other maps are safe. No new image metadata or package
format is introduced. The established RGB/RGBA8, 16:9, maximum 1280×720, 4 MiB
encoded/8 MiB decoder contract is reused; decoding is independent of the world's
1024-edge texture limit. APNG, bad dimensions/paths, missing or corrupt bytes
are refused.

The map list places a small landscape image before each name, with its existing
selection/keyboard/pointer layout retained. The selected map's image wins; only
the entry map can inherit the project/package image. Other maps get the neutral
P fallback. Bounded visible-row CPU/GPU caches upload PNGs only when the visible
images change. Menu image reading opens just the archive index and selected
PNG blob; it never loads meshes, materials, lighting, scripts or world geometry.
Corrupt images produce a named warning and the fallback without crashing
browsing. The small archive regression deliberately corrupts world semantics
and still reads the correct thumbnails, proving that separation.

Native refinement evidence: [dark room](https://chatgpt.com/api/library/files/libfile_23e1373977cc81918c98e322e3d46f66/download),
[bright snow with flashlight](https://chatgpt.com/api/library/files/libfile_f9c9f9ab2ed88191a352718149e2556b/download),
[editor image preview](https://chatgpt.com/api/library/files/libfile_7ed76396303c81918e70214a87c2888c/download),
[menu images](https://chatgpt.com/api/library/files/libfile_32634be2dcb08191891e8a4c928f7f66/download),
[corrupt-image fallback](https://chatgpt.com/api/library/files/libfile_ad0df6d5e6688191b1becf7b151c4561/download).
These refined reticle images supersede the earlier .50-alpha crosshair stills.
The native editor replay selected PNGs through visible Paste path controls; the
new SDL OS file picker was not automated through the unavailable raw-binary UI
binding. Both selection routes share the same validated attachment operation.

Final expanded source gates pass: 143 Rust regressions (0.28 seconds), strict
all-target/all-feature Clippy, locked all-feature release, formatting and 369
Python checks (98 seconds, three supported skips). Preserved final binaries
repeat all 19 crosshair captures, icon/corruption/contrast checks, the ordinary
editor windows/focus, Beam's 44 death/reset assertions and Closing Time's ten
win-route assertions. Native OS file-picker automation remains the narrow
unexecuted UI path described above; validated visible path selection passes.

Final review adds a focused loading guard: visible map PNGs are suppressed by the
same `loading_map` flag that draws the loading modal, so they cannot overdraw it.
Native menu/transition replays pass; the existing game capture queue defers its
loading-named still until the new world, so no modal still is claimed. The
initial CI run on `bd2fd97f` passed every new test but hit an existing Luau
reset/scoped-state failure (zero instead of two after a closure call).
Local full/focused checks pass; hosted scheduling against existing deadlines is
a possible cause, not an established diagnosis. Runtime budgets and later Luau safety scope are
unchanged. A follow-up CI receipt covers the loading guard.

## Final delivery receipt

Implementation commits `bd2fd97f275141735f86ec010ce245b1aa8ea42e` and
`26f5c9dcc0f00981a51f8accaa3f858de59848e1` are pushed to `0.12.0-staging`.
[CI 39112950839](https://github.com/csd113/Places/actions/runs/39112950839)
passed on the latter exact source commit: formatting, target/feature checks,
debug/release Clippy, all Rust regressions, production executables, authored
assets/generators, Python contracts and patch whitespace. The existing Luau
regression that failed in the initial run also passed; no Luau change was made.

Direct binaries, packages, source recovery, scripts, screenshots, bounded-check
logs and the asset inventory are preserved under
`local/editor-crosshair-0.12.0/`, outside `target`. No process was using
`target` at final cleanup; `cargo clean` removed 5,814 files / 5.0 GiB.
The exact preserved payload then validated the package, recovered all five
source/image files byte-for-byte, opened both recovered editor image previews,
and passed native player default-off, console-on, menu-icon, real-focus and
clean-shutdown smoke checks. `target` remains absent.

Final SHA-256: player `06837908ad862fb27cfaa979ff2b1597761fd52987dced313027090eadc024de`,
editor `923967f453eac3e8658e5588c0db0e51b477f1e04327585348cb79c32d96d1e1`,
compiler `379c33d20a49bf844cbc254a2e7b3096aae80fca10c0c870a507bc12650b3ac8`.
Launch the editor from the repository with
`./local/editor-crosshair-0.12.0/run-editor.sh`; it uses these direct binaries
and isolated state, with the repository assets and installed SDL3.

No outstanding execution blocker remains. Native acceptance is macOS/Metal
only; Windows/Linux readability remains unverified. Contrast uses the stable
rim described above, and the OS file-picker path remains unautomated.
Broad issues remain open, no main merge/release occurred, and later queue
stages were not started.
