# Arcade art direction and Bepsi vending

12 October 2026. The connected arcade, bowling and party rooms now use tighter
colored practical light pools, with a rebuilt soda machine following the actual
blue vending silhouette and front layout in the original concept board. Both
Arcade reference boards and the current birthday artwork were inspected as pixels.
The accepted birthday composition remains intact.

Two existing fixture positions now carry visible violet battens with **zero UV
excitation**. Pink/violet cabinet and pinball spill is more localized, and the
western seating pool is pink. Final native refinement restored cabinet/trim
readability after the first colored-light pass became too dark. Ordinary source
power/reach was corrected locally; ambient, exposure, bloom, atmosphere and engine
code are unchanged. Warm bowling keys and the party entrance light are preserved.

The Bepsi cabinet is **0.96 × 2.03 × 0.86 m**, with a large illuminated soda
globe/logo, two banks of twelve total drink windows, raised selection bars,
central bill/coin/return hardware and a **15 cm recessed delivery cavity**.
Only printed light panels emit; cabinet stock, payment hardware and delivery
surfaces remain ordinary. Existing localized blue source lights are preserved.
Only the decorative `vending_cat_plush` placement was removed; Spooner-man and
the reusable plush asset remain intact.

Three purposeful installed-Blender inspections covered the original silhouette
and layout, rebuilt panes/cavity/UVs, then physical selectors/materials and the
final export. Native views supplied the final lighting review. The **792-triangle,
five-primitive** model stays below the 800-triangle review ceiling; closed windows,
selectors and the real cavity account for its detail above the preferred 500.
The Rust parser caught excessive alternating material primitives during export;
grouping this static model's index runs preserves the exact material-owned
triangles and PNG while satisfying the existing engine ceiling. No limit or test
was weakened.

Artwork uses one opaque **256-square native PNG**, a 1024-square master and a
retained generated illustration. Built-in imagegen produced the soda artwork;
the [exact prompt](../../assets/references/neon_bowling_arcade/Vending%20Artwork%20Prompt.txt)
and offline fitting command are retained. No concept-board crop is a runtime
texture. The editable Blender inspection scene is
`assets/environments/arcade/props/vending_machine.blend`; the existing Python
builder remains authoritative. The revised UV contract is documented in
[Asset Specification §8.14](../ASSET_SPECIFICATION.md#814-arcade-soda-machine-revision).

The **38 birthday models, 94 placements, 32 carpet-matched trim bands, 1024 cloth
prints, 512 wraps, equal cube gifts, corrected skirt UVs, purple walls, darker
ceiling, ink masks, candle light and anchored balloon behavior** are unchanged.
All 143 protected birthday/entity/Office files and five preexisting audit files
retain their hashes. Model Zoo source changes only its vending name and width.

These are unaltered **1280×720 wgpu/Metal** captures on Apple M2 Pro, using the
same production binary, Full/High settings, frame 30 and fixed 1/60 simulation.

| Untouched baseline | Finished scene |
|---|---|
| ![Arcade before](../images/arcade-art-direction/arcade-before.png) | ![Arcade final](../images/arcade-art-direction/arcade-final.png) |
| ![Vending before](../images/arcade-art-direction/vending-before.png) | ![Bepsi final](../images/arcade-art-direction/vending-final.png) |

| Preserved wood lanes | Preserved birthday room | Connected transition |
|---|---|---|
| ![Bowling](../images/arcade-art-direction/bowling-final.png) | ![Party](../images/arcade-art-direction/party-final.png) | ![Transition](../images/arcade-art-direction/transition-final.png) |

Native checks find zero failed models and no observed floating contacts,
z-fighting or blocked passage. A held-control out-and-back route crosses all
three rooms: **168 ready samples, X=6.7–25.3497, Z=25.2, eye Y=1.6**. UV on/off
changes mean RGB by 3.04 display levels and restores **pixel-identically**.
Independent Low/High textures/filtering with Static/Full lighting also restore
pixel-identically. Two/four-second balloon views show motion with fixed ties;
selected party source IDs remain **4, 5, 14, 17**. The existing 20 m² developer
fixture verifies real candle illumination: a frosting patch gains
**20.89 / 11.02 / 2.71 RGB** with a distant control unchanged. Matched party and
cake views differ from the untouched scene by only **0.0043 / 0.0051** mean RGB.

The current normal package is **45,317,180 bytes / 43.22 MiB**, up **66,746 bytes
/ 0.15%**. Full keeps four atlas pages, 1,323 probes, 57 wall boxes and one
14,696-cell navigation region; prop batches fall 120→119 and placed prop
triangles rise by 134. Imported texture data falls 31,956→31,892 KiB after
removing the plush. The whole 336-model pack remains **104.37 MiB decoded**,
within its unchanged 106 MiB cap. Final normal bake: **524.62 seconds**, no
package warnings; current-provenance and complete package validation pass.

One bounded **240-frame** sample after 120 warmup frames per stationary view,
VSync off and no competing builds, gives arcade CPU loop mean/p95
**4.21/11.44 → 5.88/15.50 ms**. Draws/binds stay **62/14**, visible vertices
11,924→11,868, peak RSS 1,032.0→1,030.4 MiB. Final vending mean/p95 is
**6.23/15.99 ms**. These CPU timings include host/frame-pacing variation; one
pair does not establish hardware GPU cost or a general performance trend.

Formatting, strict workspace/all-target/all-feature Clippy, default Rust
tests (**2,272 library + eight integration**), all-feature Rust tests
(**2,278 + eight**) and **351 Python tests** pass; 19 existing GPU tests and
four expected Python native skips remain. Asset validation, texture/prop checks,
birthday authorer, generator freshness and asset integrity pass. Inventory:
336 GLBs, 364 embedded PNGs, 485 standalone PNGs, zero integrity errors; final
vending topology is closed and has no coincident quads. Geometry checking has
zero errors and the same 34 known differently posed bouquet-anchor warnings.
Normal tests and CI perform no production-map bakes.

[Compact evidence receipt](../images/arcade-art-direction/evidence.json) records
capture identities, hashes and measurements. Seven curated PNGs are retained.
Compiled packages stay ignored. Aero remains stale against shared catalog/tool
provenance; Office now differs only in shared catalog provenance. Their sources
are untouched. The inherited Model Zoo Full-atlas overflow was not rebaked.
Those reconciliations belong to the queued map-independence/cleanup stage.
Static retains the existing brighter/flatter approximation and distant strings
can alias. Windows/Vulkan and hardware GPU timing were not run.

Package SHA-256: `71992d9bc62cf2fa3e0a8e630d0db36ab359e23d313f2310e10dcb228e47d379`.
[Arcade branch CI](https://github.com/csd113/Places/actions?query=branch%3Aarcade-env)
and the final task handoff provide publication status and the exact commit/CI
receipt, cleanup space and custody release.
