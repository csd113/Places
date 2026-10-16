# Rust and dependency prerequisite for 0.11.0

The staging build selects exact Rust 1.99.1. Cargo's `rust-version = "1.99"`
remains the MSRV. This host reuses its matching installed stable toolchain with
an override scoped to Places; the committed pin still controls CI/releases.

All fourteen direct dependencies were checked against the official crates.io
registry on 2026-10-16, excluding prereleases and yanked releases. Four resolved
direct versions changed: glam 0.29.3 → 0.34.1, serde_json 1.0.151 → 1.0.154,
sha2 0.10.9 → 0.11.1, and zip 8.6.0 → 9.0.3. Requirements for the other ten
now name their verified latest stable release where necessary. The lock refresh
selects thirty new/updated packages in total. The highest declared dependency
MSRV is 1.90. Older transitive glam, hashbrown, miniz_oxide and syn families
remain where upstream constraints require them; no patches or forced pins were
introduced.

The glam migration selects the corresponding right-handed camera functions,
retaining each original OpenGL or zero-to-one depth convention, projection
arguments, clip correction and probe Y flip. Upstream projection arithmetic can
round differently. SHA-2 output uses the existing lowercase, zero-padded hex
formatter, including streaming executable and physics-state identities. ZIP
retains the Deflate zlib-rs backend, compression level, timestamps, permissions
and bounded archive validation. Shader bytes, physics policy, executable names,
asset resolution, package versions and authored data are unchanged. No Rust
tests were added, removed or weakened. Independent source review found no issue.

Validation on macOS, Apple M2 Pro / Metal / SDL 3.4.18:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo check --locked --workspace --all-targets --all-features` | Passed; 29.49 s |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Passed; 20.52 s |
| Same Clippy gate with `--locked --release` | Passed; 35.26 s |
| `cargo test --workspace` | 2,336 library + 7 integration passed; 19 existing ignores |
| `cargo test --locked --workspace --all-features` | 2,343 library + 7 integration passed; 19 existing ignores |
| `cargo build --locked --release --bins` | Passed; 161.81 s |
| `PLACES_SKIP_SMOKE=1 python3 -m unittest discover -s tests -p 'test_*.py'` | 363 run; 17 initial skips. All 14 geometry cases then passed with the new executable, resolving 13 skips; 4 optional skips remain |
| `python3 tools/assets/validate.py` | 593 assets, 370 placeable models, 8 themes; zero warnings |
| Independent source/version/feature review and `git diff --check` | Passed |

Rust commands used `RUSTC_WRAPPER=` for the host's sandbox-incompatible wrapper.
No shipped map was baked. The native map-compiler GUI completed `test_room`;
`validate`, `verify --require-current`, and incremental `build` passed, with
`rebuilt: false`. All twelve non-provenance package records match the baseline
byte for byte. Only tool/build provenance differs; ZIP size changed by one byte.
The native player loaded the existing Office package and displayed the no-map
menu with v0.11.0. Both apps closed normally. No permission prompt blocked these
checks, and no signing or OS permission setting was changed.

The Office captures have the same 3,024 × 1,676 dimensions and 60 ready
presentations each. Visual inspection found no scene or lighting regression;
0.2621% of pixels differ (mean absolute channel difference 0.000676 on the
0–255 scale, maximum 42), so the images are not byte-identical. Glam's changed
projection arithmetic is a source of numerical rasterization differences.
Short runs recorded median loop time 8.334 → 8.357 ms, entry-to-ready
2.882 → 5.476 s, and request-to-ready 1.841 → 1.320 s. Window creation accounted
for 0.659 → 3.964 s of those startup times. Release build wall time was
161.81 s versus the earlier stage's 135.78 s. These single, uncontrolled samples
are recorded without claiming general startup/runtime/build performance parity.

A bounded 48-frame, twelve-case physics comparison retained repeatability and
worker-count parity. Five full-state hashes changed. Temporary diagnostic copies
reproduced both original release receipts and compared 17,472 snapshot pairs:
physical poses/rotations, velocities, mass/inertia, water fractions, contacts,
impulses and render positions are bit-identical. Only render quaternion
components differ, at most two ULP; maximum normalized orientation difference
is 0.0000134°. This isolates glam's changed ARM NEON quaternion interpolation
rounding from solver behavior. No production workaround or assertion change
was needed; diagnostic instrumentation is excluded from the committed source.

Binaries, bounded package, captures, command logs, registry receipts and numerical
proof remain outside `target` in `local/dependency-upgrade-0.11.0/`. The predecessor
harness CI run [38855035823](https://github.com/csd113/Places/actions/runs/38855035823)
completed successfully. The upgrade commit's CI status is recorded separately in
the handoff receipt. Linux/Windows native execution and a controlled performance
campaign were not run. Renderer work and Rust-test removal remain later stages.

Version sources: [Rust 1.99.1 release](https://blog.rust-lang.org/2026/10/15/Rust-1.99.1/),
[glam 0.34.1](https://crates.io/crates/glam/0.34.1),
[SHA-2 0.11.1](https://crates.io/crates/sha2/0.11.1), and
[ZIP 9.0.3](https://crates.io/crates/zip/9.0.3).
