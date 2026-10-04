# Compiler audit playground

This collection retains distinct compiler regression inputs and their playable
compiled packages separately from production levels. Run from the repository root:

```sh
python3 debug-maps/compiler-audit-20261003/launch.py
python3 debug-maps/compiler-audit-20261003/launch.py <listed-alias>
python3 debug-maps/compiler-audit-20261003/launch.py <listed-alias> --quality high
```

The launcher opens the selected package directly. WASD moves, arrow keys look,
Space jumps, C crouches, E interacts, and Escape pauses. Quality profiles select
the corresponding Off, Medium or Full lighting/reflection variant; packages with
only Off reject unsupported profiles. Each alias and profile has separate runtime
state, preventing repeated map IDs from replacing another preserved fixture.

`manifest.json` records package/source hashes, available variants, original
locations and purposes. `sources/` and `packages/` contain the distinct retained
inputs. Where an original authoring file is unavailable, the source is an exact
decoded semantics snapshot, marked accordingly. Catalogue variants are retained
in `catalogues/`; the launcher selects their exact catalogue without changing the
saved asset tree. `index.py` copies and indexes newly generated evidence without
removing original files or replacing existing distinct packages.

The reuse fixtures exercise independent navigation dimensions and display
metadata across all three lighting variants. The catalogue fixtures preserve
both input catalogue snapshots and resulting packages. The last valid package
from failure tests remains playable; malformed source bytes are retained as
expected failures in the local raw evidence. A small fixture introduced during
the audit and the earlier larger fixture are both retained.

`fixture-code/` preserves deterministic Rust test definitions, including seeded
ray/triangle and BVH oracles, boundary rays captured during diagnosis, cache
connectivity, shared light layers, cancellation and worker failure. Analytic ray
tests run through Cargo; their numeric triangle generators are retained as code.
Authoring-map tests have separately playable packages here.

`bin/`, `asset-root/`, `runtime/`, `captures/`, `incremental/` and the complete raw
`regressions/` tree are locally preserved and Git-ignored. They are outside
`target` and temporary directories. Saved build/asset manifests record their
hashes, and licenses are in `licenses/`. The launcher uses the saved native Mac
game and SDL library when present; another checkout can build with
`cargo build --release` and use repository assets instead.

For a deterministic native capture, choose a new output path:

```sh
python3 debug-maps/compiler-audit-20261003/launch.py <listed-alias> \
  --capture debug-maps/compiler-audit-20261003/captures/<new-name>.png
```

Frame 1 freezes authored animation brightness. `tools/bench/capture_compiler.py`
compares baseline and optimized production packages through the same native
renderer, preserving isolated settings, exact package identity logs, load traces
and pixel comparisons. The compiler benchmark evidence is retained in
`tools/bench/results/compiler-audit-20261003/`; raw timing samples, failed
optimization candidates and diagnostic builds are preserved there.
