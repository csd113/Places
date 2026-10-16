# Agent Notes

## Map and Environment Authoring

Before creating or substantially modifying any map, level file, or map asset
(texture, material, decal, prop, or light placement), read
`docs/MAP_AUTHORING_GUIDE.md`. It is the canonical reference for the currently
implemented level format, asset catalog, materials, textures, props, lighting
and validation. Do not author maps from memory or from design documents alone.

## Asset Specification

Before creating, replacing, resizing or converting any visual asset
(texture, decal, fixture face, normal map, emissive mask, model texture,
thumbnail or icon), read `docs/ASSET_SPECIFICATION.md`. It is the canonical
specification for asset classes, required aspect ratios, preferred source
resolutions, alpha and tiling contracts, orientation, and file location.

Preserve every mandatory part of an asset's contract (aspect ratio, UV
layout, orientation, alpha behaviour). Do not infer an asset's contract from
its current PNG dimensions. If an asset class is not covered, inspect the
implementation and update the specification before introducing the asset.

## Texture Asset Policy

All textures used by the project must exist as real `.png` files in the
appropriate repository asset directory under `assets/`; source code
loads and references those files. This is a repository-wide requirement:

- Texture assets may be created ahead of time or sourced appropriately,
  but the resulting PNG files must be committed to the repository in the
  correct asset location.
- Do not generate texture imagery procedurally from Rust, Python,
  shaders, embedded pixel arrays, drawing commands, or any other source
  code at runtime.
- Do not recreate a texture on demand when the application starts or
  when a level loads; load the corresponding PNG asset instead.
- When adding a new textured surface, fixture, decal, sign, material, or
  similar visual asset, create or add its PNG texture first and
  reference that asset from the implementation.
- Existing procedural/code-generated textures encountered during
  relevant work should be migrated to proper PNG assets where practical
  rather than extending the procedural approach.
- 3D model geometry is not affected by this rule, but textures and
  material images used by those models should still be stored as normal
  asset files where applicable.

## Linting Standards (Clippy)

This project enforces a strict Clippy configuration, declared in
Cargo.toml under [lints.clippy] (or [workspace.lints.clippy] for the
workspace root). See that file for the authoritative, current list —
do not duplicate it here, just follow it.

Before considering any coding task complete, you must:
1. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   and confirm it exits with status 0.
2. Run `cargo build --locked --release --workspace --all-features` and confirm
   the production executables build successfully.
3. If justified Rust tests are added in future, run `cargo test --workspace`
   as well. The 0.11.0 cleanup removed all existing project-owned Rust tests.

Rules for fixing lint violations:
- Fix the root cause (proper Result/Option handling, checked/saturating
  arithmetic, TryFrom casts, Box on oversized enum variants, references
  instead of needless clones) rather than suppressing the lint.
- Do not silence a lint with #[allow(clippy::...)] as a first resort.
  Only use it for a genuine false positive at that specific call site,
  scoped as narrowly as possible, with a one-line comment explaining why
  it's safe.
- Never add a blanket #![allow(clippy::all)] / #![allow(clippy::pedantic)]
  at the crate root, and never lower a lint's configured level in
  Cargo.toml to make a failure go away.
- New code must be clean against this lint set from the start.
- If a fix needs a larger refactor than the current task justifies, leave
  a `// TODO(clippy):` comment explaining what's deferred, but the build
  must still pass -D warnings via a real fix or a justified narrow allow.

## Test Value and Cost

Every future test must demonstrate essential, concrete regression value: name
the behavior or bug it protects, its unique value beyond existing coverage, and
its proportional runtime and resource cost before adding or expanding it.
Use the narrowest suitable deterministic fixture. Do not add tests that mirror
implementation details, duplicate broad coverage, or inflate test counts without
improving confidence in the game.

Compilation, default Rust tests and automatic CI must not depend on baking a full
shipped map. Full-map builds, native visual checks and performance campaigns belong
in explicit developer tooling, outside the Rust test suite. Preserve meaningful
format, security, numerical-boundary and regression checks with small fixtures.
Rust tests must live under the owning `src/engine/tests/`, `src/game/tests/`,
`src/editor/tests/` or `src/map_compiler/tests/` folder. Wire them into the owning
module with `#[cfg(test)] mod tests;` and a `tests/mod.rs` (or an explicit
`#[path = "tests/..."]` declaration) so Cargo actually discovers them; merely
placing a file under `src/` does not register a test. Do not create an editor
module or scaffolding until editor work is authorized. Keep test-only helpers
and fixtures with their owner. Production validation, security checks and useful
developer diagnostics remain production code.

Prune redundant or costly tests instead of hiding them behind ignore markers or
feature flags. Ordinary CI must never require full map baking. Test maintenance
must leave practical time to run and visually inspect the game.

## Performance Evidence

All FPS performance comparisons must disable V-sync and every frame limiter,
record the actual presentation mode and physical drawable size, and verify that
display pacing is not capping the result. A run plateauing at refresh rate even
after requesting Immediate mode is inconclusive for throughput gains; label it
accordingly. Requested settings alone do not establish an uncapped measurement.
