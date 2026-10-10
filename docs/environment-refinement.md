# Environment refinement — October 10, 2026

Beach, Frutiger Aero and Winter retain their existing lighting, textures, animations and exploration. This pass fixes construction and topology at their sources rather than adding general polygon detail.

## What changed

Beach: ramp sides close only against the actual adjacent surface, removing buried shoreline crossbands while preserving exposed exterior skirts. The cove uses one sealed scalloped foam strip. A closed masonry arcade grounds the raised lookout terrace, and bunting connects real facade anchors. Shallows fish sit 20 cm higher, with slightly clearer authored water; their full swim poses remain below the surface and above the seabed.

Aero: the dome soffit fills the square atrium corners, curved white spandrels close stock above both lime portals, and modular bays keep rounded apertures inside square outer frames. Returning bay stock clears the broad face; leaf marks follow the folded panes. Glass keeps its existing approximate transparency and sheen.

Winter: measured roof stock seats all ridge caps; each pitched side has a single closed snow blanket instead of intersecting bay caps and eave strips. Door-hood snow sinks into its timber support. Cottage strings hang from short fascia arms with visible upstands; lodge rail clips intersect their supporting timber. Existing roof thickness, icicles, warm bulbs, collision and accessible interiors are retained.

The Model Zoo adds four exhibits in previously unused annex slots. All 271 earlier prop objects and every non-prop field are unchanged.

## Play the maps and use the button

Open **Beach**, **Frutiger Aero**, or **Winter** in the ordinary player. Authored sources and runnable packages are the corresponding `assets/levels/beach_demo`, `frutiger_aero_demo`, and `winter` JSON / `.placesmap` pairs.

In Winter, enter the raised lodge west of the central square via its ramp and front doorway. Inside, the timber-backed control panel is beside the front door at X −10.25, Z −10.34. Aim within 2.1 m and press **E** (default Interact). The main **Blizzard control**, centre world Y 1.89 m, alternates moderate strength 0.35 and calm over two seconds. Three lower rockers select mild 0.15, moderate 0.35 or severe 1.0. The upper **Timed snowfall** rocker starts a gentle 120-second cycle between calm and 0.45: 45-second dwells and 15-second ramps. Pressing it again eases to calm. Any manual weather action cancels the timer. The main lever and start/stop prompts show control state; preset prompts name the next action rather than the active strength. Holding E produces one activation.

Fresh loads/reset restore authored calm with the timer off. Pause freezes strength and timer controls; live Low/Medium/High changes retain strength, target and cycle. The reusable optional `weather_alternate`, `weather_cycle`, `set_weather_strength`, `set_weather_cycle`, `toggle_weather` and `set_prompt` contracts use the established interaction and weather pipeline. Numeric wind, snow, fog and sky parameters blend in place with retained travel history, seeds, textures and shelter coefficients. No map reload, lighting bake or new weather renderer is involved.

Winter's saved 2048×1024 sky keeps its original aurora. A narrow offline U-edge correction changes at most 4/255 in only the outer 16 columns per side; all 2016 interior columns, dimensions and PNG contract are unchanged. All four catalog panoramas have exact left/right edge matches; V poles stay separate.

## Native evidence and validation

The native review covers all 17 Beach and 18 Aero High composition views, their focused Low/Medium counterparts, all 12 Winter views at each quality, and all four skies across qualities and U-wrap/pole cameras. The final player reproduces six original hero images byte for byte. Actual controller routes cover 49 Beach/Aero traversals and 1,933 loaded samples, including ramps, shore/swim exits, portals, stairs and obstruction boundaries. Winter's lodge approach activates the physical control through actual movement and aiming. Near-facade moving views cover both cottages and the lodge; final exterior roof sweeps supplement them. Geometry tests independently check every authored roof and light assembly.

Seven native weather scenarios pass a float32-aware independent oracle: preset changes, forty presses, held E through quality changes, a full timer cycle, manual override/priming, pause/resume and same-id reload. Prepared capacity stays at 1,400 particles, 5,600 vertices, one group and one texture, with zero capacity growth. Visible severe samples submit 55/700, 86/1,050 and 136/1,400 flakes at Low/Medium/High. Retained traveled phases are rescaled with changing weather dimensions, preventing the moving particle sheets caught during acceptance. Explicit small aim boxes keep the five controls independently selectable.

Formatting and both strict Clippy profiles pass, including the required `cargo clippy --workspace --all-targets --all-features -- -D warnings`. The required `cargo test --workspace` passes (2,231 library tests, zero failures; binaries, integrations and documentation also pass). The asset/geometry/tool suite passes all 362 Python tests; the explicit atlas-quality test and both actual Low GPU resource tests pass. The 52-case map matrix has 50 passing packages, one intended CPU contract and one expected named invalid-geometry rejection. An initial all-features run exposed a stale geometry-revision-8 test expectation; its revision-9 assertion was corrected and passed. Final package provenance and exact-head CI receipts accompany publication.

Raw logs, scripts and recovery evidence remain under ignored `tools/bench/results/environment-refinement-20261010`; only the following 17 original native PNGs are in documentation.

| Composition | Before | After |
| --- | --- | --- |
| Beach shoreline | [View](images/environment-refinement/beach-shoreline-before.png) | [View](images/environment-refinement/beach-shoreline-after.png) |
| Beach town | [View](images/environment-refinement/beach-town-wide-before.png) | [View](images/environment-refinement/beach-town-wide-after.png) |
| Aero atrium | [View](images/environment-refinement/aero-hero-before.png) | [View](images/environment-refinement/aero-hero-after.png) |
| Aero construction | [View](images/environment-refinement/aero-architecture-before.png) | [View](images/environment-refinement/aero-architecture-after.png) |
| Winter cottage | [View](images/environment-refinement/winter-cottage-before.png) | [View](images/environment-refinement/winter-cottage-after.png) |
| Winter lodge | [View](images/environment-refinement/winter-lodge-before.png) | [View](images/environment-refinement/winter-lodge-after.png) |
| Winter composition | [View](images/environment-refinement/winter-overview-before.png) | [View](images/environment-refinement/winter-overview-after.png) |

[Control panel](images/environment-refinement/winter-controls.png) · [Severe outside](images/environment-refinement/winter-severe-outside.png) · [Sheltered doorway](images/environment-refinement/winter-severe-sheltered.png). Cameras and settings match within each pair. Beach/Aero use fixed 1/60-second time and frame 60; Winter uses its preserved 1.5-second capture recipe, with an immediate overview. Calm flakes are not pixel-locked in the Winter pairs.

## Measurements and limits

Matched native Metal measurements use the local Apple M2 Pro, High quality, 640×360 logical / 1280×720 drawable, bloom and vsync off, fixed 1/60 simulation, 120 warmup frames and 600 measured frames with GPU completion. These are single environment pairs; display pacing varies, so lower means do not establish a general speedup. Draws/triangles are renderer counters, not a device-wide census.

| View | Mean frame ms, before → after | p95 ms, before → after | Draw calls | Visible triangles |
| --- | --- | --- | --- | --- |
| Beach hero | 8.830 → 7.986 | 20.052 → 19.414 | 89 → 85 | 19,466 → 18,118 |
| Aero hero | 7.686 → 6.839 | 16.463 → 16.232 | 83 → 84 | 18,546 → 20,278 |
| Aero corridor | 7.626 → 7.004 | 16.115 → 17.018 | 62 → 62 | 8,468 → 9,236 |
| Winter lodge | 8.327 → 6.767 | 19.319 → 16.508 | 140 → 146 | 41,557 → 42,488 |

Hero-room mean first rose 17.4%. Two focused alternating-order repeats measured +1.2% and −2.7%, with identical geometry counters. Across all three pairs, median run means are 6.911 → 7.160 ms and median p95 is 16.656 → 17.195 ms: the large initial increase did not repeat consistently, but a small regression cannot be excluded. The contact view measured 6.635 → 6.839 ms in its single pair.

Vertex buffers change by −1.8% Beach, +7.5% Aero and −0.6% Winter. The Aero cost buys fitted bay/portal stock rather than decorative subdivision. Peak process RSS changes from 1,050.3 → 1,037.2 MB Beach, 623.0 → 625.0 MB Aero hero, and 1,078.8 → 1,117.4 MB Winter. RSS is a whole-process peak and does not isolate toggle allocations.

| Package | Before → after, decimal MB | Change | Final normal compile |
| --- | --- | --- | --- |
| Beach | 43.285 → 32.365 | −25.23% | 199.1 s |
| Aero | 26.350 → 26.401 | +0.20% | 131.2 s |
| Winter | 81.338 → 80.039 | −1.60% | 297.8 s |

All seven shipping packages pass integrity, inspection and `verify --require-current`. The normal compiler ran serially with 12 workers, preserving Off/Medium/Full variants. These compile times are costs, not matched speed comparisons. A final compiler-provenance refresh changed only package metadata; every other entry retained identical bytes.

Forty presses and the other native scenarios retain weather buffer/texture capacities without growth. Whole-process heap allocation and GPU-residency traces are **unavailable**: Instruments waited on macOS authorization before the player entered main. Only the task-owned stalled recorder and suspended player were stopped; system security settings were unchanged. The diagnostic evidence is preserved. Resource-capacity stability is verified; a zero-heap-allocation claim is not made.

Fine close-range shoreline risers, approximate Aero alpha/glass and mild ceiling/contact variation remain. Low quality has flatter lighting and can fragment very thin fixtures or show rectangular path-light patches; the review did not establish these as new regressions. Winter intentionally exposes a narrow dark shingle band below its seated ridge caps. The lower preset rockers use aim prompts rather than permanent labels. Moving roof frames are sampled rather than a continuous flicker recording. The views and routes do not prove every camera or collision path.

A runnable Apple Silicon macOS player, compiler, real assets and isolated launch script are preserved outside Cargo target in the local evidence directory’s `final-tools` and `final-runnable` folders. SDL3 remains the native system dependency. The target/cache and task-owned sleep assertion are retained for the authorized queued handoff; the final queued owner performs cleanup.
