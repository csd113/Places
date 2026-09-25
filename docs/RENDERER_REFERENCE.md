# Renderer reference: the preserved implementation

The project's former GLES2 renderer is preserved in Git at the
`renderer-gles2-reference` tag (commit
`797370e17aab1409a5de3ea70b9a68f742452`), a lightweight tag on a side commit
whose parent is `034f8e371572613e24f26c2eb64de3f345bfc4c6`. The tag exists in
the local repository only; it is not pushed. Mainline has no dependency on it;
this document is only the map.

## What the snapshot contains

- `src/render/opengl/**` — the complete former renderer: programs, context
  policy, framebuffers, post-processing, reflections and captures.
- `src/render/backend.rs` — the renderer selector that existed at the time.
- `src/render/common/**`, `src/render/facade.rs` — the renderer-neutral layer
  and the facade as they stood, next to the wgpu renderer in `src/render/wgpu/**`.
- The renderer audit and records, `docs/renderer-baseline/` (25 Full + 25 Low
  captures), `tools/bench/**`, `tools/verify.sh`, the Python suites and the
  Cargo manifests.

It excludes the 62 user-owned binary asset edits the capture-time tree carried
(33 prop/entity `.glb` files and 29 `.png` textures/thumbnails, including the
1024×1024 `white_01.png`). In the snapshot those files are byte-identical to
the earlier revision `4ad6d97` — the asset tree `docs/renderer-baseline/` was
captured from; the user's asset work remains in mainline history.

## How to inspect it

```sh
git worktree add /tmp/places-reference renderer-gles2-reference
cd /tmp/places-reference
cargo build --release
PLACES_RENDERER=opengl PLACES_LEVEL=places_demo target/release/places
```

The snapshot predates the removal of the renderer selector, so it still accepts
`PLACES_RENDERER=opengl`. Against its own committed asset tree,
`tools/bench/compare_baseline.py` reports 50/50 byte-identical images.
`docs/renderer-baseline/` (in mainline and in the snapshot) is the frozen
reference set; it is never regenerated.
