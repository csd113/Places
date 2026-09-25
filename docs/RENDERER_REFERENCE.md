# Renderer reference: the preserved GLES2 + wgpu state

Stage 11 removed the OpenGL/GLES2 renderer from mainline. The complete
end-of-Stage-10 dual-renderer state — the OpenGL reference renderer, the
feature-complete wgpu renderer, the renderer-neutral common layer, the parity
tooling and the frozen baseline — is preserved in Git at:

| field | value |
| --- | --- |
| tag | `renderer-gles2-reference` |
| commit | `797370e17aab1409a5de3ea70b9a17068f742452` |
| parent | `034f8e371572613e24f26c2eb64de3f345bfc4c6` — "mostly complete wgpu migration" |
| location | local repository only; the tag is not pushed |

The tag is a lightweight tag on a side commit whose parent is the mainline
commit the Stage 10 working tree was committed as. Mainline continues from
`034f8e3`; the reference commit is reachable through the tag.

## Why it exists

The OpenGL renderer was the visual and behavioural reference the wgpu port was
built and measured against. Deleting it from mainline keeps the engine simple,
but the renderer itself remains valuable: as the ground truth for the parity
numbers, as a source of exact reference behaviour, and as archaeology if a
future port needs to consult it. The Git reference is the preservation
mechanism; this document is the map to it.

## What the snapshot contains

* `src/render/opengl/**` — the complete OpenGL/GLES2 renderer: GL programs and
  GLSL sources, context policy, framebuffers, post-processing, reflections,
  captures.
* `src/render/backend.rs` — the temporary `PLACES_RENDERER` selector.
* `src/render/common/**`, `src/render/facade.rs` — the renderer-neutral layer
  and the dual-backend facade as they stood at Stage 10.
* `src/render/wgpu/**` — the complete wgpu renderer.
* `docs/RENDERER_AUDIT.md`, `docs/WGPU_*.md`, `docs/renderer-baseline/` (25
  Full + 25 Low OpenGL captures), `tools/bench/**`, `tools/verify.sh`, the
  Python suites and the Cargo manifests.

## What it excludes

The 62 pre-existing user-owned binary asset edits the Stage 10 working tree
carried — 33 prop/entity `.glb` files and 29 `.png` textures/thumbnails,
including the production 1024x1024 `white_01.png`. In the snapshot those files
are byte-identical to the pre-migration revision `4ad6d97`, which is the asset
tree `docs/renderer-baseline/` was captured from. The user's asset edits are not
lost: they remain in mainline at `034f8e3`. Three `assets/**/README.md` text
updates (the `Liminal` -> `Places` rename and a dimension correction that
matches the historical sheet too) stay with the migration.

## How to inspect it

```sh
git worktree add /tmp/places-reference renderer-gles2-reference
cd /tmp/places-reference
cargo build --release
PLACES_RENDERER=opengl PLACES_LEVEL=places_demo target/release/places
```

The canonical baseline reproduces from the snapshot's own asset tree:

```sh
PLACES_CAPTURE_DIR="$PWD/target/agent-work/reference-captures" \
    sh tools/bench/capture_baseline_views.sh
python3 tools/bench/compare_baseline.py target/agent-work/reference-captures
#   PASS high: 25 byte-identical images
#   PASS low:  25 byte-identical images
```

The wgpu renderer from the snapshot was also re-verified against the Stage 10
canonical capture set: per-view mean differences at most 0.001/255, largest
channel difference 2/255, 0 % of pixels over the 8/255 threshold.

## Canonical baseline

`docs/renderer-baseline/` (in both mainline and the snapshot) is the frozen
Stage 0 OpenGL reference: 25 views in Full and 25 in Low, with
`BASELINE.md` describing the capture conditions. It is not regenerated.

## Parity evidence (Stage 10, macOS/Metal)

* Canonical 50-view gate: smallest per-view mean difference 0.103/255, largest
  0.854/255, mean of the 50 means 0.4189/255, largest share of pixels over
  8/255: 0.213 % (`docs/WGPU_STAGE10.md` §3).
* Lightmap atlas pages byte-identical between renderers; probe, planar, lightmap
  and bloom contributions match in magnitude, maximum and spatial correlation
  (§4–§5).
* Remaining differences are bounded backend/raster/minification behaviour, not
  missing features (§7–§8, §12).

## Outstanding validation

macOS/Metal is verified. Linux/Vulkan and Windows/D3D12 were **not executed**
(environment blocked) and remain an explicit Stage 12 gate; nothing in the
Stage 10 or Stage 11 record claims otherwise.
