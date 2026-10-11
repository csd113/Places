# Developer orientation and repository cleanup

Places keeps authored content, developer controls and generated output separate.
Start at the root README to run the player, then use these ownership boundaries.

| Directory | Contents |
| --- | --- |
| `src/` | Rust player, offline compiler and engine modules |
| `assets/shared/` | Shared surface sheets, fitted decals, static props and UI atlas |
| `assets/environments/<theme>/` | Environment textures, decals, props and source masters |
| `assets/entities/<id>/` | Rigged GLBs and standalone texture sheets |
| `assets/references/` | Immutable concept art and authoring prompts; excluded from exports |
| `assets/catalog.json` | Logical asset/material registry; paths are relative to `assets/` |
| `levels/` | Nine maintained source maps and their explicit `lineup.txt` allowlist |
| `tests/fixtures/levels/` | Focused movement, lighting, geometry, entity and capacity controls |
| `tests/fixtures/native/` | Camera, controller and reference-image inputs |
| `tools/{levels,textures,props,entities}/` | Offline authoring generators and checks |
| `tools/bench/` | Explicit compiler, loading and native diagnostic campaigns |
| `docs/` | Current contracts, architecture and verification; `reports/` holds dated findings |
| `docs/images/` | Curated original documentation images |

Read [map authoring](MAP_AUTHORING_GUIDE.md) before changing a map and
[asset contracts](ASSET_SPECIFICATION.md) before changing artwork or models.
[Environment profiles](assets/ENVIRONMENTS.md) and [entity profiles](assets/ENTITIES.md)
consolidate kit-specific instructions formerly spread across 24 asset READMEs.
Prop sheets stay beside their GLBs; logical `core:` and theme IDs are unchanged.

## Build from source

Install the [desktop prerequisites](../README.md#desktop-prerequisites). The
repository toolchain is pinned in `rust-toolchain.toml`.

```sh
cargo build --locked --release
sh tools/build-maps.sh --workers 8
./target/release/places --list-levels
./target/release/places
```

The map command compiles exactly `levels/lineup.txt` through `places-compile`,
then verifies provenance, archive validity and all Off/Medium/Full variants.
Packages go to ignored `assets/levels/`. Compiler capture/bake work is offline
and may take several minutes per large map. Use a native GPU for normal captures.
A source-only player opens its menu; compile the maps or import a package to play.
There is no package baked into the executable.

After changing just one source, use an explicit output path:

```sh
./target/release/places-compile build levels/winter.json \
  --out assets/levels/winter.placesmap --asset-root assets --workers 8
python3 tools/levels/compile.py --check
```

Run a generator's `--check` before editing its owned output. For generated maps,
edit the generator and regenerate the JSON; the [map index](../levels/README.md)
lists each owner. [Offline tooling](OFFLINE_TOOLING.md) documents the asset pipeline.
Textures are committed PNG inputs; the player never generates texture artwork.

## Maintained maps and regression controls

The playable sources are `places_demo`, `model_zoo`, `pool_showcase`,
`home_showcase`, `lantern_hollow`, `winter`, `beach_demo`, `frutiger_aero_demo`
and `retro_arcade_demo`.
The first eight sources were retained during repository cleanup; Retro Arcade
adds the reference-led bowling and arcade showcase. Home and Pool
are compact standalone showcases; Places Demo also contains their latest detailed
refinements. Model Zoo is retained as the complete model/animation reference.

The catalog has eight environments: Office, Pool, Home, Outdoor, Winter, Beach,
Frutiger Aero and Arcade. Office appears in Places Demo and has an asset diagnostic,
but lacks a dedicated showcase. That gap is a feature-request candidate; no
replacement environment was constructed during cleanup.

Obsolete local dev maps, historical compiler/renderer review snapshots and their
public discovery paths were retired. Eleven superseded Hero-stage source maps
and the old asset-polish showroom were replaced, where needed, by small
`material_transport` and `architecture_before`/`architecture_after` fixtures.
Movement Test moved into the [regression fixture index](../tests/fixtures/levels/README.md).
Geometry, water/ice, collision, entity and capacity assertions remain supported.
Severe Winter controls derive from the single maintained Winter source and an
explicit weather configuration rather than maintaining a second village.

Native diagnostic tools consume explicitly compiled fixtures. For example:

```sh
mkdir -p target/verification/maps
./target/release/places-compile build tests/fixtures/levels/movement_test.json \
  --out target/verification/maps/tests__fixtures__levels__movement_test.placesmap \
  --asset-root assets --workers 8
python3 tools/bench/capture_movement_test.py --case step_ \
  --out target/verification/movement-steps
```

The fixture index and `tools/bench/` help name the additional inputs for complete
campaigns. Diagnostic packages do not belong in the curated export directory.

## Verification and export

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'
python3 tools/assets/validate.py
```

Rust tests use bounded deterministic fixtures and require no existing `.placesmap`
files. They do not compile or bake full shipped maps. Test additions must justify
unique behavior coverage and runtime/resource cost; prefer the smallest fixture
that reproduces a concrete regression. The test profile preserves debug assertions
and overflow checks.

Automatic GitHub checks run source contracts and separate, timed Rust format,
check, debug/release Clippy and fixture-test stages. They do not compile shipped
maps, capture their reflection probes or run native game campaigns. Run those
explicitly on the desktop:
`python3 tools/levels/compile.py --map retro_arcade_demo --workers 8` compiles and
validates one authored map. `sh tools/verify.sh` is the full desktop gate, including
the compiled lineup, package tests and native smoke checks. See
[verification](VERIFICATION.md) for the explicit diagnostic gates.

After compiling current maps, export with `sh tools/package.sh OUTPUT_DIRECTORY`.
It copies only the curated packages and runtime asset directories, excluding
concept references, source masters and arbitrary local maps. Destination checks reject the repository root, its ancestors, home-root
destinations and product symlinks. Distribution contains generated
packages, while Git contains their sources and required assets.

## Cleanup preservation

The cleanup preserves every preexisting tracked PNG/GLB byte and all eight kept
map sources. Asset moves repair the catalog, loaders, tests, schemas, generators
and native manifests together. Final reports and curated images remain readable;
bulk receipts and raw logs stay in ignored verification directories.
The documentation image set is curated to 69 originals; 80 repeated campaign
frames are preserved under `local/retained/repo-cleanup-20261011/docs/images/`.
Native golden images and immutable concept references remain intact.

All 73 formerly tracked compiled packages are removed from the current Git tree
and globally ignored. Useful local packages, historical debug bundles and unique
evidence are retained under ignored `local/retained/`; existing caches and native
deliverables remain available for the separate storage audit. No history or LFS
object purge is part of this cleanup. Fresh checkouts require none of those local
recovery files.

Cleanup acceptance covers all eight normal Off/Medium/Full builds, archive hashes,
authored climate, native resource loading and the portable export. An isolated
source-only tree passes source checks and compiles a normal Home package that
matches the repository build byte for byte. Preserved Demo, Zoo, Hollow, Winter
and Beach packages retain identical geometry, lightmaps, collision and reflection
records after accounting for moved model paths. The initial eight-map build took
about 63 minutes on the native verification host with eight workers; subsequent
unchanged builds verify and reuse their outputs. The final handoff records the
published commit, exact-head CI and local gate results.
