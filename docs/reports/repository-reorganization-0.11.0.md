# 0.11.0 stage 1: repository and source responsibilities

The existing crate now has three explicit owners: `engine`, `game` and
`map_compiler`. Application entry points are thin. Engine production code has
no dependency on game UI or Map Compiler orchestration. No editor, renderer
rewrite, dependency or workspace was introduced. See [ARCHITECTURE.md](../ARCHITECTURE.md)
for the dependency direction and future editor contract.

The game owns the player loop, input policy, settings, menus, console and HUD.
The engine owns reusable scene/package records, asset resolution, rendering,
physics, entity simulation, targeting and counters. The Map Compiler owns
source preparation, offline navigation, preprocessing, validation, lighting,
package publication and its retained CLI/GUI. Shared progress observers and
locomotion values remove reverse imports; static collision compilation no longer
constructs the game's live entity world. Duplicate bounds math and asset-profile
validation now have one implementation each.

Repository cleanup removed superseded constructors, authoring scripts, an unused
codec experiment and a retired capture script. Useful report/asset documentation
was consolidated. The used package repacker remains a tool with a thin entry
point. Branding has one byte-identical PNG source. Export staging now includes
the runtime `gameplay.json` that it previously omitted. The itemized paths,
destinations, reasons and final rename-aware patch totals are in
[CHANGELOG.md](../../CHANGELOG.md).

All existing Rust tests remain for the separately ordered post-renderer test
cleanup. No Rust audit file was added. Root audit modules remain test-only;
their names refer to geometry/lighting regressions, not production subsystems.
Existing opt-in compiler/renderer diagnostics retain active consumers.

Validation on macOS/Metal:

| Command or check | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo check --locked --workspace --all-targets --all-features` | Passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo clippy --locked --release --workspace --all-targets --all-features -- -D warnings` | Passed |
| `cargo test --workspace` | 2,336 library + 7 integration checks passed; 19 existing ignores |
| `cargo test --workspace --all-features` | 2,343 library + 7 integration checks passed; 19 existing ignores |
| `cargo build --locked --release --bins` | Passed; 135.78 s wall time |
| `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'` | 361 run, 4 existing skips, no failures; 121.093 s |
| Asset validation, accepted-model check, Home validator, entity clip boundaries | Passed; 593 assets, 370 placeable models, 8 themes; zero asset warnings |
| Independent source/path/diff review | Passed; include paths resolve, 10 WGSL files unchanged, Rust test inventory preserved |

Rust commands used `RUSTC_WRAPPER=` to bypass the host's sandbox-incompatible
sccache wrapper. Ordinary source checks performed no whole-map bake. Python Zoo
tests retain their assertions with tiny static/animated fixtures; one obsolete
constructor-only Python test was removed.

Native acceptance used preserved 0.10.0 and final release binaries. Empty-install
startup displayed the menu and v0.11.0, created normal state and required no map.
The Office capture at the same pose/settings was exactly equal across all
2,560 × 1,440 pixels, with 60 ready presentations and no missed ready frames.
Recorded entry-to-first-scene times were 2.761 s before and 2.787 s after.
These short capture runs are not a controlled frame-time campaign; activation
delays affected the launch waiter, and no broad runtime or cold-build performance
claim is made. Linux and Windows native execution were not exercised.

The real Map Compiler GUI completed the bounded `test_room.json` fixture,
cancelled another attempt, then completed a retry. Actual receiver counts were
visible during progress. The 1,946,140-byte package passed `validate`,
`verify --require-current`, and an incremental `build` returned `rebuilt: false`.
All twelve non-provenance archive records matched the 0.10.0 fixture byte for
byte. Differences were the tool identity, version and their manifest hashes;
authored/package format semantics were unchanged. No shipped map was rebaked.

Ignored cleanup removed 394 redundant/regenerable files (20,001,488 bytes),
including four hash-verified asset duplicates. Unique art, editable sources,
settings, deliveries and release evidence were retained. Logs, comparison
receipts, the bounded package and final binaries are preserved outside `target`
under `local/stage1-0.11.0/`; this is local evidence, not a release payload.

Stage 2 can work within `engine/render` and the existing simulation/presentation
handoff. GPU ownership, synchronization, z-fighting, sound and editor behavior
remain separate work. This stage does not publish 0.11.0 or merge staging to main.
